use bytes::Bytes;
use http_body_util::{BodyExt, Full};

pub static NODE_STATUS_ONLINE: &str = "online";
pub static NODE_STATUS_OFFLINE: &str = "offline";

type GenericError = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, GenericError>;
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}
