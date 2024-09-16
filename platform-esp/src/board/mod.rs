cfg_if::cfg_if! {
    if #[cfg(feature = "target-lilygo_t_deck")] {
        mod lilygo_t_deck;
        pub use lilygo_t_deck::*;
    } else if #[cfg(feature = "target-lilygo_t_watch_s3")] {
        mod lilygo_t_watch_s3;
        pub use lilygo_t_watch_s3::*;
    } else {
        compile_error!("no target support feature enabled");
    }
}
