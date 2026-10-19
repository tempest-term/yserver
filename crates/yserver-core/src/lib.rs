#[cfg(feature = "native")]
pub mod anon_shm;
pub mod backend;
pub mod composite_redirects;
pub mod core_loop;
pub mod crossings;
pub(crate) mod dri3;
#[cfg(feature = "native")]
pub mod host_x11;
#[cfg(not(feature = "native"))]
#[path = "embedded_host.rs"]
pub mod host_x11;
pub mod nested;
pub mod present_scheduler;
pub mod properties;
pub mod randr;
pub mod resources;
pub mod server;
pub mod transport;
#[cfg(feature = "native")]
mod unix_fd;
pub mod xauth;
pub mod xinput;

#[cfg(feature = "embedded")]
mod memory_stream;
#[cfg(feature = "embedded")]
pub use memory_stream::MemoryStream;
