// Repository abstraction layer
// Provides a unified data access interface with pluggable storage backends

mod traits;
mod sqlite;
mod backend;

pub mod repository {
    pub use traits::*;
    pub use sqlite::*;
    pub use backend::*;
}
