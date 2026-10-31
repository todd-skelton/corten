# Testing

Test through the router. No server or browser is needed:

```rust
#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(req: Request<Body>) -> String {
        let res = app().router().oneshot(req).await.unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn greeting_is_a_patch_for_the_runtime() {
        let req = Request::post("/greet")
            .header("corten", "action")   // what the runtime sends for a data-patch form
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("name=Ada"))
            .unwrap();
        assert_eq!(send(req).await, r#"<p id="greeting" aria-live="polite">Hello, Ada!</p>"#);
    }
}
```

1. Cover each mode a handler serves: no `corten` header (the whole document), `corten: action`
   (the patch), and `accept: application/json` (data screens).
2. Keep the scripts' declarations current with a test, so `tsc --noEmit` or the editor checks
   the scripts against the Rust types and `#[event]` classes (feature `types`):

   ```rust
   #[test]
   fn script_types_match_the_server() {
       corten::types::Types::new()
           .add::<Stock>()
           .components(&[LivePrices])
           .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
   }
   ```

3. A typed seam that must reject bad input gets a `compile_fail` doctest.
4. Behavior that runs in the browser needs a browser check: load the page and use it, once with
   JavaScript and once without.
5. A test that needs a database: code that writes takes the connection (`&mut PgConnection`),
   not the pool, so a test can pass it a transaction. Prefer `#[sqlx::test]` (a fresh database per
   test, migrations applied, dropped after); a transaction rolled back at the end works against a
   shared database but cannot test code that commits on its own. Mark tests that need a database
   the machine may not have `#[ignore = "needs DATABASE_URL"]` (`cargo test -- --ignored`). A
   fixture recorder that calls a live service is a binary or an example, never a test. See the app
   guide, Testing an app.
