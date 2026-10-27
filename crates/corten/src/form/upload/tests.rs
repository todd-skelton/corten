use super::*;
use crate::form::Errors;
use axum::body::Body;

#[derive(serde::Deserialize, corten_macros::Fields, Debug, PartialEq)]
struct Listing {
    title: String,
    price: Option<u32>,
    #[serde(default)]
    tags: Vec<String>,
    photos: Files,
}

const BOUNDARY: &str = "----corten7MA4YWxkTrZu0gW";

/// A part: its name, file name, content type and content.
type Part<'a> = (&'a str, Option<&'a str>, Option<&'a str>, &'a [u8]);

/// A body as a browser writes it.
fn body(parts: &[Part<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, filename, content_type, content) in parts {
        out.extend_from_slice(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes());
        if let Some(filename) = filename {
            out.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if let Some(content_type) = content_type {
            out.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(content);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    out
}

/// The request for `bytes`, arriving in pieces of `size`.
fn request(bytes: &[u8], size: usize) -> Request {
    let pieces: Vec<Result<Bytes, std::io::Error>> = bytes.chunks(size.max(1)).map(|c| Ok(Bytes::copy_from_slice(c))).collect();
    Request::builder()
        .method("POST")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from_stream(futures_util::stream::iter(pieces)))
        .unwrap()
}

/// What a handler reads: each file's field, name, type and content, then the fields.
async fn read(req: Request, limit: Option<usize>) -> crate::Result<(Vec<(String, String, String, Vec<u8>)>, Listing)> {
    let mut upload = Upload::<Listing>::from_request(req, &()).await.map_err(|_| Error::new("rejected"))?;
    if let Some(limit) = limit {
        upload.limit(limit);
    }
    let mut files = Vec::new();
    while let Some(mut file) = upload.next_file().await? {
        let mut content = Vec::new();
        while let Some(piece) = file.chunk().await? {
            content.extend_from_slice(&piece);
        }
        files.push((file.field().to_string(), file.filename().to_string(), file.content_type().to_string(), content));
    }
    Ok((files, upload.fields().await?))
}

fn status(error: Error) -> StatusCode {
    error.into_response().status()
}

#[tokio::test]
async fn files_and_fields_read_whatever_the_pieces() {
    // A boundary-like run inside a file, and CRLFs, are the file's.
    let photo: &[u8] = b"\x89PNG\r\n\r\n--not the boundary\r\n----corten7MA4YWxkTrZu0g\r\nend";
    let bytes = body(&[
        ("title", None, None, "Charizard ✨".as_bytes()),
        ("photos", Some("front.png"), Some("image/png"), photo),
        ("tags", None, None, b"holo"),
        ("photos", Some("C:\\Users\\me\\back.jpg"), Some("image/jpeg"), b""),
        ("tags", None, None, b"1st"),
        ("price", None, None, b""),
    ]);
    for size in 1..=bytes.len() {
        let (files, listing) = read(request(&bytes, size), None).await.unwrap_or_else(|e| panic!("pieces of {size}: {e:?}"));
        assert_eq!(
            files,
            vec![
                ("photos".into(), "front.png".into(), "image/png".into(), photo.to_vec()),
                ("photos".into(), "back.jpg".into(), "image/jpeg".into(), Vec::new()),
            ],
            "pieces of {size}"
        );
        assert_eq!(
            listing,
            Listing { title: "Charizard ✨".into(), price: None, tags: vec!["holo".into(), "1st".into()], photos: Files },
            "pieces of {size}"
        );
    }
}

#[tokio::test]
async fn an_empty_file_input_sends_no_file_and_fields_read_past_files() {
    let bytes = body(&[
        ("photos", Some(""), Some("application/octet-stream"), b""),
        ("photos", Some("a.png"), None, b"abc"),
        ("title", None, None, b"Pikachu"),
        ("price", None, None, b"12"),
    ]);
    // The handler takes no file: `fields` reads past it.
    let upload = Upload::<Listing>::from_request(request(&bytes, 7), &()).await.ok().unwrap();
    let listing = upload.fields().await.unwrap();
    assert_eq!(listing, Listing { title: "Pikachu".into(), price: Some(12), tags: vec![], photos: Files });
    let (files, _) = read(request(&bytes, 7), None).await.unwrap();
    assert_eq!(files, vec![("photos".into(), "a.png".into(), "application/octet-stream".into(), b"abc".to_vec())]);
}

#[tokio::test]
async fn a_file_over_its_limit_is_none_and_the_form_still_reads() {
    let bytes = body(&[("photos", Some("big.png"), Some("image/png"), &[7u8; 5000]), ("title", None, None, b"Mew")]);
    let mut upload = Upload::<Listing>::from_request(request(&bytes, 512), &()).await.ok().unwrap();
    let f = Listing::FIELDS;
    let mut errors = Errors::new();
    while let Some(mut file) = upload.next_file().await.unwrap() {
        assert!(file.is(f.photos));
        if file.bytes_up_to(4096).await.unwrap().is_none() {
            errors.add(f.photos, "Each photo can be up to 4 KB.");
        }
    }
    assert_eq!(upload.fields().await.unwrap().title, "Mew");
    assert_eq!(errors.get(f.photos), Some("Each photo can be up to 4 KB."));
    let mut upload = Upload::<Listing>::from_request(request(&bytes, 512), &()).await.ok().unwrap();
    let mut file = upload.next_file().await.unwrap().unwrap();
    assert_eq!(file.bytes_up_to(5000).await.unwrap().map(|b| b.len()), Some(5000));
}

#[tokio::test]
async fn the_body_limit_is_a_413_and_the_default_is_2_mib() {
    let bytes = body(&[("photos", Some("big.png"), None, &[0u8; 3000]), ("title", None, None, b"Mew")]);
    assert_eq!(status(read(request(&bytes, 1000), Some(2048)).await.unwrap_err()), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(read(request(&bytes, 1000), Some(4096)).await.is_ok());
    let big = body(&[("photos", Some("big.png"), None, &vec![0u8; DEFAULT_LIMIT]), ("title", None, None, b"Mew")]);
    assert_eq!(status(read(request(&big, 64 << 10), None).await.unwrap_err()), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn fields_that_do_not_fit_are_a_422() {
    let bytes = body(&[("title", None, None, b"Mew"), ("price", None, None, b"lots")]);
    assert_eq!(status(read(request(&bytes, 64), None).await.unwrap_err()), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_body_that_is_not_multipart_is_a_415_and_a_broken_one_a_400() {
    let req = Request::builder()
        .method("POST")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("title=Mew"))
        .unwrap();
    let rejected = Upload::<Listing>::from_request(req, &()).await.err().unwrap();
    assert_eq!(rejected.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let whole = body(&[("title", None, None, b"Mew")]);
    let broken: [&[u8]; 4] = [
        // Ends inside a part.
        &whole[..whole.len() - 12],
        // No closing boundary.
        &whole[..whole.len() - 4 - BOUNDARY.len() - 2],
        b"--no boundary here at all",
        // A part with no name.
        format!("--{BOUNDARY}\r\nContent-Disposition: form-data\r\n\r\nx\r\n--{BOUNDARY}--\r\n").leak().as_bytes(),
    ];
    for bytes in broken {
        let error = read(request(bytes, 5), None).await.unwrap_err();
        assert_eq!(status(error), StatusCode::BAD_REQUEST, "{}", String::from_utf8_lossy(bytes));
    }
}

#[test]
fn boundaries_and_parameters_read_as_written() {
    assert_eq!(boundary("multipart/form-data; boundary=abc"), Some("abc"));
    assert_eq!(boundary("Multipart/Form-Data; charset=utf-8; BOUNDARY=\"a b\""), Some("a b"));
    assert_eq!(boundary("multipart/mixed; boundary=abc"), None);
    assert_eq!(boundary("multipart/form-data"), None);
    assert_eq!(boundary(&format!("multipart/form-data; boundary={}", "x".repeat(71))), None);
    let head = parse_head(
        b"Content-Disposition: form-data; name=\"a%22b\"; filename=\"say \\\"hi\\\".txt\"\r\nContent-Type: text/plain",
    )
    .ok()
    .unwrap();
    assert_eq!(
        (head.name.as_str(), head.filename.as_deref(), head.content_type.as_deref()),
        ("a\"b", Some("say \"hi\".txt"), Some("text/plain"))
    );
    let head = parse_head(b"content-disposition: form-data; name=title").ok().unwrap();
    assert_eq!((head.name.as_str(), head.filename), ("title", None));
}
