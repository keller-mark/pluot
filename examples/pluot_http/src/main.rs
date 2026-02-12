#[macro_use] extern crate rocket;

// TODO: see https://api.rocket.rs/v0.5/rocket/response/stream/struct.ByteStream

#[get("/<name>/<age>")]
fn hello(name: &str, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/hello", routes![hello])
}