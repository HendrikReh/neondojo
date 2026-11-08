use utoipa::OpenApi;

fn main() {
    println!(
        "{}",
        bgtutor::api::ApiDoc::openapi()
            .to_pretty_json()
            .expect("OpenAPI serialization")
    );
}
