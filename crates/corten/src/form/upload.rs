//! Reading a form posted as `multipart/form-data`: its files one at a time as they arrive, and
//! its text fields by [`Form`](super::Form)'s rules. See [`Upload`].

use std::{borrow::Cow, marker::PhantomData};

use axum::{
    body::{BodyDataStream, Bytes},
    extract::{FromRequest, Request},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;

use super::FieldName;
use crate::Error;

/// The whole body's size unless the handler sets another ([`Upload::limit`]): axum's default
/// for a body.
const DEFAULT_LIMIT: usize = 2 * 1024 * 1024;

/// How much of a body past its limit is read past (not kept) before the answer, so the browser,
/// still sending, reads the answer instead of a broken connection. Past this the connection is
/// closed.
const READ_PAST: usize = 64 * 1024 * 1024;

type Result<T, E = UploadError> = std::result::Result<T, E>;

/// Why an [`Upload`] could not be read. `?` turns it into an [`Error`] with its status (the
/// error page). Any read can find the body over its limit, so a handler that answers that with
/// the form reads in one block and matches [`UploadError::TooLarge`] once:
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(serde::Deserialize, Fields)]
/// struct NewListing {
///     title: String,
///     photos: Files,
/// }
///
/// async fn create(mut upload: Upload<NewListing>) -> corten::Result<String> {
///     upload.limit(60 << 20);
///     let read = async {
///         let mut photos = Vec::new();
///         while let Some(mut file) = upload.next_file().await? {
///             photos.push(file.bytes_up_to(10 << 20).await?);
///         }
///         Ok::<_, UploadError>((upload.fields().await?, photos))
///     };
///     match read.await {
///         Ok((listing, photos)) => Ok(format!("{}: {} photos", listing.title, photos.len())),
///         Err(UploadError::TooLarge { .. }) => Ok("The photos are over 60 MB together.".into()),
///         Err(other) => Err(other.into()),
///     }
/// }
/// # let _: Methods = post(create);
/// ```
#[derive(Debug)]
pub enum UploadError {
    /// The body is larger than [`Upload::limit`] (413). What the form sent is not read; the
    /// rest of the body is read past, up to 64 MiB, so the browser receives the answer.
    TooLarge { limit: usize },
    /// The body breaks the multipart format, or could not be read to its end (400).
    Malformed(String),
    /// The text fields do not fit the form struct (422).
    Fields(String),
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadError::TooLarge { limit } => write!(f, "the upload is larger than its limit of {limit} bytes"),
            UploadError::Malformed(what) => write!(f, "the multipart body is malformed: {what}"),
            UploadError::Fields(what) => write!(f, "Failed to deserialize form body: {what}"),
        }
    }
}

impl From<UploadError> for Error {
    fn from(error: UploadError) -> Self {
        let status = match error {
            UploadError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            UploadError::Malformed(_) => StatusCode::BAD_REQUEST,
            UploadError::Fields(_) => StatusCode::UNPROCESSABLE_ENTITY,
        };
        Error::new(error.to_string()).with_status(status)
    }
}

/// The most a part's headers may take.
const HEAD_LIMIT: usize = 8 * 1024;

/// A form posted as `multipart/form-data` (`<form enctype="multipart/form-data">` with an
/// `<input type="file">`): its files one at a time as they arrive, then its text fields read into
/// `T` as [`Form`](super::Form) reads them. A file input is named by a [`Files`] field of `T`, so
/// markup, the files and [`Errors`](super::Errors) take its [`FieldName`]:
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(serde::Deserialize, Fields)]
/// struct NewListing {
///     title: String,
///     photos: Files,
/// }
///
/// async fn create(mut upload: Upload<NewListing>) -> corten::Result<String> {
///     let f = NewListing::FIELDS;
///     upload.limit(60 << 20);
///     let (mut photos, mut errors) = (Vec::new(), Errors::new());
///     while let Some(mut file) = upload.next_file().await? {
///         if !file.is(f.photos) {
///             continue;
///         }
///         match file.bytes_up_to(10 << 20).await? {
///             Some(bytes) => photos.push((file.filename().to_string(), bytes)),
///             None => errors.add(f.photos, "Each photo can be up to 10 MB."),
///         }
///     }
///     let listing = upload.fields().await?;
///     Ok(format!("{}: {} photos", listing.title, photos.len()))
/// }
/// # let _: Methods = post(create);
/// ```
///
/// The body is read as the handler asks for it, and nothing it skips is kept: a file the loop
/// passes over is read past, not stored, and [`File::chunk`] hands a file on piece by piece (to an
/// object store, say) without holding it. Text fields are kept for [`Upload::fields`], which reads
/// what is left. A file input left empty sends no file. The whole body may be 2 MiB unless the
/// handler sets [`Upload::limit`] before reading; past it the request is a 413. A body that is not
/// multipart is a 415, one that breaks the format a 400, and fields that do not fit `T` a 422.
pub struct Upload<T> {
    parts: Parts,
    fields: Vec<(String, String)>,
    form: PhantomData<fn() -> T>,
}

/// A file input of a form struct: `photos: Files` names the input (`name={f.photos}`) and its
/// errors (`errors.add(f.photos, ..)`); its files come from [`Upload::next_file`], so it holds
/// nothing itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Files;

impl<'de> serde::Deserialize<'de> for Files {
    /// Missing (as it always is: files are not text fields) or anything else, it is the marker.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Marker;
        impl<'de> serde::de::Visitor<'de> for Marker {
            type Value = Files;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a file input")
            }
            fn visit_none<E>(self) -> Result<Files, E> {
                Ok(Files)
            }
            fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Files, D::Error> {
                <serde::de::IgnoredAny as serde::Deserialize>::deserialize(d).map(|_| Files)
            }
        }
        deserializer.deserialize_option(Marker)
    }
}

impl<T> Upload<T> {
    /// The most the whole body may take, files and fields together (2 MiB unless set); set it
    /// before reading. The limit for one file is the handler's ([`File::bytes_up_to`]).
    pub fn limit(&mut self, bytes: usize) -> &mut Self {
        self.parts.limit = bytes;
        self
    }

    /// The next file, reading the text fields before it (kept for [`Upload::fields`]); `None`
    /// once the body has no more. What is left of the file before is read past.
    pub async fn next_file(&mut self) -> Result<Option<File<'_>>> {
        loop {
            let Some(head) = self.parts.head().await? else { return Ok(None) };
            match head.filename {
                None => {
                    let value = self.parts.text().await?;
                    self.fields.push((head.name, value));
                }
                // A file input left empty sends a part with no file name and nothing in it.
                Some(name) if name.is_empty() => {}
                Some(filename) => {
                    let content_type = head.content_type.unwrap_or_else(|| "application/octet-stream".into());
                    return Ok(Some(File { parts: &mut self.parts, field: head.name, filename, content_type }));
                }
            }
        }
    }

    /// The text fields read into `T` as [`Form`](super::Form) reads them, after reading the rest
    /// of the body (files it reaches are read past).
    pub async fn fields(mut self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        while self.next_file().await?.is_some() {}
        let pairs = self.fields.iter().map(|(k, v)| (Cow::Borrowed(k.as_str()), Cow::Borrowed(v.as_str()))).collect();
        super::de::from_pairs(pairs).map_err(|error| UploadError::Fields(error.to_string()))
    }
}

impl<T, S: Send + Sync> FromRequest<S> for Upload<T> {
    type Rejection = Response;

    async fn from_request(req: Request, _: &S) -> Result<Self, Response> {
        let boundary = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).and_then(boundary);
        let Some(boundary) = boundary else {
            return Err(Error::new("Upload requests must have Content-Type: multipart/form-data with a boundary")
                .with_status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                .into_response());
        };
        let mut delimiter = b"\r\n--".to_vec();
        delimiter.extend_from_slice(boundary.as_bytes());
        let parts = Parts {
            body: req.into_body().into_data_stream(),
            // The first boundary has no line break before it: one is put there, and what comes
            // before it is read as a part that is never handed out.
            buf: Bytes::from_static(b"\r\n"),
            delimiter: delimiter.into(),
            read: 0,
            limit: DEFAULT_LIMIT,
            ended: false,
            state: State::Body,
        };
        Ok(Upload { parts, fields: Vec::new(), form: PhantomData })
    }
}

/// The `boundary` of a `multipart/form-data` content type: 1 to 70 characters (RFC 2046).
fn boundary(content_type: &str) -> Option<&str> {
    let mut params = content_type.split(';');
    if !params.next()?.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    let value = params.find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim().eq_ignore_ascii_case("boundary").then_some(value.trim())
    })?;
    let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
    (1..=70).contains(&value.len()).then_some(value)
}

/// A file of an [`Upload`], read piece by piece ([`File::chunk`]) or whole up to a size
/// ([`File::bytes_up_to`]). What is not read is read past when the next file is asked for.
pub struct File<'a> {
    parts: &'a mut Parts,
    field: String,
    filename: String,
    content_type: String,
}

impl File<'_> {
    /// Whether the file came from the input `field` names, at the form's top, in a group or in a
    /// row.
    pub fn is<P: super::Place>(&self, field: FieldName<P>) -> bool {
        field == self.field.as_str()
    }

    /// The name of the input it came from.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The file's name as the browser sent it, without a path: the user's to choose, so never a
    /// path to write to as it is.
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// The content type the browser sent (from the file's extension, so the client's word, not a
    /// check); `application/octet-stream` when it sent none.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// The file's next piece as it arrives, `None` at its end.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>> {
        self.parts.data().await
    }

    /// The whole file, or `None` when it is larger than `max` bytes (the rest is read past, so
    /// the form's other fields can still be read and the field marked).
    pub async fn bytes_up_to(&mut self, max: usize) -> Result<Option<Bytes>> {
        let mut pieces: Vec<Bytes> = Vec::new();
        let mut len = 0;
        while let Some(piece) = self.chunk().await? {
            len += piece.len();
            if len > max {
                while self.chunk().await?.is_some() {}
                return Ok(None);
            }
            pieces.push(piece);
        }
        Ok(Some(match pieces.len() {
            0 => Bytes::new(),
            1 => pieces.swap_remove(0),
            _ => Bytes::from(pieces.concat()),
        }))
    }
}

/// The body, split at its boundaries as it arrives.
struct Parts {
    body: BodyDataStream,
    /// What has arrived and not been handed out.
    buf: Bytes,
    /// `\r\n--{boundary}`: what ends a part.
    delimiter: Box<[u8]>,
    read: usize,
    limit: usize,
    ended: bool,
    state: State,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// In a part's content.
    Body,
    /// Just past a delimiter: another part's headers, or the end.
    After,
    /// Past the closing delimiter.
    Done,
}

/// A part's headers.
struct Head {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
}

fn malformed(what: &str) -> UploadError {
    UploadError::Malformed(what.into())
}

impl Parts {
    /// Reads one more piece of the body onto `buf`; false at its end.
    async fn more(&mut self) -> Result<bool> {
        if self.read > self.limit {
            return Err(UploadError::TooLarge { limit: self.limit });
        }
        if self.ended {
            return Ok(false);
        }
        match self.body.next().await {
            None => {
                self.ended = true;
                Ok(false)
            }
            Some(Err(error)) => Err(UploadError::Malformed(format!("it could not be read: {error}"))),
            Some(Ok(piece)) => {
                self.read += piece.len();
                if self.read > self.limit {
                    // A browser still sending when the answer comes takes the closed connection
                    // for a network error and never shows the answer: read past the rest first.
                    let mut past = self.read - self.limit;
                    while past <= READ_PAST {
                        match self.body.next().await {
                            Some(Ok(piece)) => past += piece.len(),
                            _ => break,
                        }
                    }
                    self.ended = true;
                    self.buf = Bytes::new();
                    return Err(UploadError::TooLarge { limit: self.limit });
                }
                // Usually nothing is left over (see `data`), so the piece is used as it came.
                self.buf = if self.buf.is_empty() { piece } else { Bytes::from([&self.buf[..], &piece[..]].concat()) };
                Ok(true)
            }
        }
    }

    /// The current part's next piece, `None` at its end (or when not in a part).
    async fn data(&mut self) -> Result<Option<Bytes>> {
        loop {
            if self.state != State::Body {
                return Ok(None);
            }
            if let Some(at) = find(&self.buf, &self.delimiter) {
                if at > 0 {
                    return Ok(Some(self.buf.split_to(at)));
                }
                self.buf = self.buf.slice(self.delimiter.len()..);
                self.state = State::After;
                return Ok(None);
            }
            // Everything but an end that could begin the delimiter is the part's.
            let held = partial(&self.buf, &self.delimiter);
            if self.buf.len() > held {
                return Ok(Some(self.buf.split_to(self.buf.len() - held)));
            }
            if !self.more().await? {
                return Err(malformed("it ended inside a part"));
            }
        }
    }

    /// The current part's content as text.
    async fn text(&mut self) -> Result<String> {
        let mut bytes = Vec::new();
        while let Some(piece) = self.data().await? {
            bytes.extend_from_slice(&piece);
        }
        String::from_utf8(bytes).map_err(|_| malformed("a field is not UTF-8"))
    }

    /// The next part's headers, reading past the rest of the current part; `None` at the end.
    async fn head(&mut self) -> Result<Option<Head>> {
        while self.data().await?.is_some() {}
        if self.state == State::Done {
            return Ok(None);
        }
        // The delimiter's line: `--` closes the body, else (past any spaces) a line break.
        let end = loop {
            if self.buf.starts_with(b"--") {
                self.state = State::Done;
                return Ok(None);
            }
            if let Some(end) = find(&self.buf, b"\r\n\r\n").filter(|_| self.buf.len() >= 2) {
                break end;
            }
            if self.buf.len() > HEAD_LIMIT {
                return Err(malformed("a part's headers are too long"));
            }
            if !self.more().await? {
                return Err(malformed("it ended before its closing boundary"));
            }
        };
        let line = find(&self.buf, b"\r\n").unwrap_or(end);
        if !self.buf[..line].iter().all(|b| *b == b' ' || *b == b'\t') {
            return Err(malformed("a boundary is followed by text"));
        }
        let headers = if end > line { &self.buf[line + 2..end] } else { &[][..] };
        let head = parse_head(headers)?;
        self.buf = self.buf.slice(end + 4..);
        self.state = State::Body;
        Ok(Some(head))
    }
}

/// A part's `Content-Disposition: form-data; name=".."` (with `filename=".."` for a file) and
/// `Content-Type`.
fn parse_head(bytes: &[u8]) -> Result<Head> {
    let text = std::str::from_utf8(bytes).map_err(|_| malformed("a part's headers are not UTF-8"))?;
    let (mut disposition, mut content_type) = (None, None);
    for line in text.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else { return Err(malformed("a part's header has no name")) };
        if name.trim().eq_ignore_ascii_case("content-disposition") {
            disposition = Some(value.trim());
        } else if name.trim().eq_ignore_ascii_case("content-type") {
            content_type = Some(value.trim().to_string());
        }
    }
    let disposition = disposition.ok_or_else(|| malformed("a part has no Content-Disposition"))?;
    let mut params = Params(disposition);
    if !params.next_token().is_some_and(|kind| kind.eq_ignore_ascii_case("form-data")) {
        return Err(malformed("a part is not form-data"));
    }
    let (mut name, mut filename) = (None, None);
    while let Some((key, value)) = params.next_param()? {
        if key.eq_ignore_ascii_case("name") {
            name = Some(value);
        } else if key.eq_ignore_ascii_case("filename") {
            filename = Some(value);
        }
    }
    let name = name.ok_or_else(|| malformed("a part has no name"))?;
    // A path some browsers once sent, and some clients still do: the file's own name is the last part.
    let filename = filename.map(|f| f.rsplit(['/', '\\']).next().unwrap_or_default().to_string());
    Ok(Head { name, filename, content_type })
}

/// The parameters of a header value: `form-data; name="photo"; filename="a b.png"`.
struct Params<'a>(&'a str);

impl<'a> Params<'a> {
    /// The value's first token, before any `;`.
    fn next_token(&mut self) -> Option<&'a str> {
        let (token, rest) = self.0.split_once(';').unwrap_or((self.0, ""));
        self.0 = rest;
        Some(token.trim()).filter(|t| !t.is_empty())
    }

    /// The next `key=value` or `key="value"`. A quoted value is read as browsers write it (a
    /// quote, CR and LF as `%22`, `%0D`, `%0A`, a backslash as itself, so an old browser's
    /// `C:\path\name` stays a path) and as other clients write a quote (`\"`).
    fn next_param(&mut self) -> Result<Option<(&'a str, String)>> {
        let rest = self.0.trim_start_matches([' ', '\t', ';']);
        if rest.is_empty() {
            return Ok(None);
        }
        let (key, rest) = rest.split_once('=').ok_or_else(|| malformed("a header parameter has no value"))?;
        let key = key.trim();
        let rest = rest.trim_start();
        let (value, rest) = if let Some(quoted) = rest.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = quoted.char_indices();
            let end = loop {
                match chars.next() {
                    Some((i, '"')) => break i + 1,
                    // `\"` is a quote, unless no quote follows to end the value (`"C:\dir\"`).
                    Some((i, '\\')) if quoted[i + 1..].starts_with('"') && quoted[i + 2..].contains('"') => {
                        value.push('"');
                        chars.next();
                    }
                    Some((_, c)) => value.push(c),
                    None => return Err(malformed("a quoted header parameter does not end")),
                }
            };
            let value = value.replace("%22", "\"").replace("%0D", "\r").replace("%0A", "\n");
            (value, &quoted[end..])
        } else {
            let (value, rest) = rest.split_once(';').unwrap_or((rest, ""));
            (value.trim().to_string(), rest)
        };
        self.0 = rest;
        Ok(Some((key, value)))
    }
}

/// Where `needle` first begins in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let first = needle[0];
    let mut from = 0;
    while let Some(i) = hay.get(from..)?.iter().position(|b| *b == first) {
        let at = from + i;
        let candidate = hay.get(at..at + needle.len())?;
        if candidate == needle {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// How long the end of `hay` is that could be the start of `needle` (shorter than all of it).
fn partial(hay: &[u8], needle: &[u8]) -> usize {
    let start = hay.len().saturating_sub(needle.len() - 1);
    (start..hay.len()).find(|&at| needle.starts_with(&hay[at..])).map_or(0, |at| hay.len() - at)
}

#[cfg(test)]
mod tests;
