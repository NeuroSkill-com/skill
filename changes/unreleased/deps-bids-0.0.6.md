### Dependencies

- **bids-rs 0.0.6 from crates.io**: the BIDS export crates (`bids-core`, `bids-io`, `bids-eeg`, `bids-validate`, `bids-layout`) now come from crates.io at 0.0.6 instead of the bids-rs git `main` branch (0.0.4), so builds no longer depend on that branch. 0.0.6 adds no new audit findings (0.0.5 would have brought in `rsa` and an old `rustls-webpki`); `bids-schema` now uses `rustls-graviola` for TLS. Export behaviour is unchanged.
