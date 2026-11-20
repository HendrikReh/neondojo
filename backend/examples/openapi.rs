use utoipa::OpenApi;

fn main() {
    println!(
        "{}",
        neondojo::api::ApiDoc::openapi()
            .to_pretty_json()
            .expect("OpenAPI serialization")
    );
}
