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

/// A file input in a group: named `proof.scan`, and never a text field.
#[derive(serde::Deserialize, corten_macros::Fields, Debug, PartialEq, Default)]
struct Proof {
    scan: Files,
}

#[derive(serde::Deserialize, corten_macros::Fields, Debug, PartialEq)]
struct Claim {
    title: String,
    #[fields(group)]
    #[serde(default)]
    proof: Proof,
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
    for size in 1..=bytes.len() {
        let (files, _) = read(request(&bytes, size), None).await.unwrap_or_else(|e| panic!("pieces of {size}: {e:?}"));
        let expected = vec![("photos".into(), "a.png".into(), "application/octet-stream".into(), b"abc".to_vec())];
        assert_eq!(files, expected, "pieces of {size}");
    }
}

/// A part with bytes is a file even with an empty file name (a client may send none): it is
/// handed out whole, with an empty name, wherever the pieces split it, even when its bytes begin
/// as the delimiter that would end it does.
#[tokio::test]
async fn a_part_with_bytes_and_no_file_name_is_a_file() {
    let bytes = body(&[
        ("photos", Some(""), Some("image/png"), b"\r\n----corten7MA4YWxkTrZu0g\r\n"),
        ("photos", Some(""), None, b""),
        ("title", None, None, b"Eevee"),
    ]);
    for size in 1..=bytes.len() {
        let (files, listing) = read(request(&bytes, size), None).await.unwrap_or_else(|e| panic!("pieces of {size}: {e:?}"));
        assert_eq!(
            files,
            vec![("photos".into(), String::new(), "image/png".into(), b"\r\n----corten7MA4YWxkTrZu0g\r\n".to_vec())],
            "pieces of {size}"
        );
        assert_eq!(listing.title, "Eevee", "pieces of {size}");
    }
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
async fn a_file_input_in_a_group_is_named_under_the_group() {
    let f = Claim::FIELDS;
    assert_eq!(f.proof.scan.to_string(), "proof.scan");
    let bytes = body(&[("title", None, None, b"Damaged"), ("proof.scan", Some("dent.jpg"), Some("image/jpeg"), b"jpg")]);
    let mut upload = Upload::<Claim>::from_request(request(&bytes, 9), &()).await.ok().unwrap();
    let mut file = upload.next_file().await.unwrap().unwrap();
    assert!(file.is(f.proof.scan));
    assert_eq!(file.bytes_up_to(16).await.unwrap().as_deref(), Some(&b"jpg"[..]));
    assert!(upload.next_file().await.unwrap().is_none());
    assert_eq!(upload.fields().await.unwrap(), Claim { title: "Damaged".into(), proof: Proof { scan: Files } });
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
async fn a_body_over_its_limit_is_too_large_on_every_read_and_read_to_its_end() {
    let bytes = body(&[("photos", Some("big.png"), None, &[0u8; 3000]), ("title", None, None, b"Mew")]);
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = seen.clone();
    let pieces = bytes
        .chunks(500)
        .map(move |c| {
            counted.fetch_add(c.len(), std::sync::atomic::Ordering::Relaxed);
            Ok::<_, std::io::Error>(Bytes::copy_from_slice(c))
        })
        .collect::<Vec<_>>();
    let req = Request::builder()
        .method("POST")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from_stream(futures_util::stream::iter(pieces)))
        .unwrap();
    let mut upload = Upload::<Listing>::from_request(req, &()).await.unwrap();
    upload.limit(1024);
    let mut file = upload.next_file().await.unwrap().unwrap();
    assert!(matches!(file.bytes_up_to(5000).await, Err(UploadError::TooLarge { limit: 1024 })));
    // The browser sent the rest, which was read past, so it can read the answer.
    assert_eq!(seen.load(std::sync::atomic::Ordering::Relaxed), bytes.len());
    assert!(matches!(upload.next_file().await, Err(UploadError::TooLarge { .. })));
    assert!(matches!(upload.fields().await, Err(UploadError::TooLarge { .. })));
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

#[derive(crate::Route)]
#[route("/api")]
struct ImportRoot;

#[derive(crate::Route)]
#[route("/api/import")]
struct Import;

#[derive(crate::Route)]
#[route("/api/listings")]
struct Listings;

/// A file-only route's input, named by its one field.
#[derive(serde::Deserialize, corten_macros::Fields)]
struct Export {
    file: Files,
}

/// A JSON route that takes one file and no text fields.
async fn import(mut upload: Upload<Export>) -> crate::Result<String> {
    let mut sizes = Vec::new();
    while let Some(mut file) = upload.next_file().await? {
        if file.is(Export::FIELDS.file) {
            sizes.push(file.bytes_up_to(16).await?.map_or(0, |b| b.len()));
        }
    }
    Ok(format!("{sizes:?}"))
}

/// A JSON route whose text fields must fit `Listing`.
async fn list(upload: Upload<Listing>) -> crate::Result<String> {
    Ok(upload.fields().await?.title)
}

/// An API's own failure shape, `{"error": ..}`.
#[cfg(feature = "json")]
#[derive(crate::ToJson)]
struct Refusal {
    error: String,
}

/// The status, content type and body an API under `/api` answers `req` with.
async fn api_call(errors: bool, req: Request) -> (StatusCode, String, String) {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let router = crate::App::new(crate::Routes::from(axum::Router::new()))
        .api::<ImportRoot>(|api| {
            let api = api
                .route::<Import>(crate::Methods::writes(axum::routing::post(import)))
                .route::<Listings>(crate::Methods::writes(axum::routing::post(list)));
            #[cfg(feature = "json")]
            let api = if errors { api.errors(|_, message| Refusal { error: message.to_string() }) } else { api };
            #[cfg(not(feature = "json"))]
            let _ = errors;
            api
        })
        .router();
    let res = router.oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let content_type = parts.headers.get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    (parts.status, content_type, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

fn to_import(content_type: &str, bytes: Vec<u8>) -> Request {
    to("/api/import", content_type, bytes)
}

fn to(uri: &str, content_type: &str, bytes: Vec<u8>) -> Request {
    Request::builder().method("POST").uri(uri).header(header::CONTENT_TYPE, content_type).body(Body::from(bytes)).unwrap()
}

/// Under an API, `Upload` is taken as an argument and its refusals are the API's failures: a body
/// that is not multipart, one that breaks the format and one over the limit answer as Problem
/// Details, as a `Json<T>` that does not parse does, and so do fields that do not fit.
#[tokio::test]
async fn under_an_api_an_uploads_refusals_are_the_apis_failures() {
    let multipart = format!("multipart/form-data; boundary={BOUNDARY}");
    let (status, _, body) = api_call(false, to_import(&multipart, self::body(&[("file", Some("a.csv"), None, b"a,b")]))).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "[3]"), "the file is read by its input's name");
    let (status, content_type, body) = api_call(false, to_import("application/json", b"{}".to_vec())).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(content_type, "application/problem+json", "{body}");
    assert!(body.starts_with(r#"{"title":"Unsupported Media Type","status":415,"detail":"Upload requests must have"#), "{body}");
    let broken = format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.csv\"\r\n\r\na,b");
    let (status, content_type, body) = api_call(false, to_import(&multipart, broken.into_bytes())).await;
    assert_eq!((status, &content_type[..]), (StatusCode::BAD_REQUEST, "application/problem+json"), "{body}");
    assert!(body.contains("the multipart body is malformed"), "an UploadError the handler returns with ?: {body}");
    let large = vec![b'x'; 3 << 20];
    let over = self::body(&[("file", Some("a.csv"), None, &large)]);
    let (status, content_type, body) = api_call(false, to_import(&multipart, over)).await;
    assert_eq!((status, &content_type[..]), (StatusCode::PAYLOAD_TOO_LARGE, "application/problem+json"), "{body}");
    assert!(body.contains(r#""status":413"#), "over the 2 MiB default: {body}");
    let unfit = self::body(&[("title", None, None, b"Mew"), ("price", None, None, b"lots")]);
    let (status, content_type, body) = api_call(false, to("/api/listings", &multipart, unfit)).await;
    assert_eq!((status, &content_type[..]), (StatusCode::UNPROCESSABLE_ENTITY, "application/problem+json"), "{body}");
}

/// An API with an error shape of its own answers an upload's refusal in that shape.
#[cfg(feature = "json")]
#[tokio::test]
async fn an_apis_own_error_shape_holds_for_an_upload() {
    let (status, content_type, body) = api_call(true, to_import("text/csv", b"a,b".to_vec())).await;
    assert_eq!((status, &content_type[..]), (StatusCode::UNSUPPORTED_MEDIA_TYPE, "application/json"));
    assert!(body.starts_with(r#"{"error":"Upload requests must have"#), "{body}");
    // And the refusals the handler returns with `?`: malformed, over the limit, fields that do not fit.
    let multipart = format!("multipart/form-data; boundary={BOUNDARY}");
    let broken = format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.csv\"\r\n\r\na,b");
    let large = vec![b'x'; 3 << 20];
    let unfit = self::body(&[("title", None, None, b"Mew"), ("price", None, None, b"lots")]);
    for (req, expected) in [
        (to_import(&multipart, broken.into_bytes()), StatusCode::BAD_REQUEST),
        (to_import(&multipart, self::body(&[("file", Some("a.csv"), None, &large)])), StatusCode::PAYLOAD_TOO_LARGE),
        (to("/api/listings", &multipart, unfit), StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        let (status, content_type, body) = api_call(true, req).await;
        assert_eq!((status, &content_type[..]), (expected, "application/json"), "{body}");
        assert!(body.starts_with(r#"{"error":"#), "{body}");
    }
}
