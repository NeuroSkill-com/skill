#[cfg(test)]
mod tests {
    use crate::sample::{now, Sample, StreamHandle, StreamIter};
    use std::sync::mpsc;

    // ── Sample::zeroed ────────────────────────────────────────────────────────

    #[test]
    fn zeroed_creates_correct_channel_count() {
        for n in [1, 4, 8, 16, 24] {
            let s = Sample::zeroed(n);
            assert_eq!(s.eeg.len(), n, "eeg length mismatch for n={n}");
            assert!(s.eeg.iter().all(|&v| v == 0.0), "eeg not zero for n={n}");
        }
    }

    #[test]
    fn zeroed_optional_fields_are_none() {
        let s = Sample::zeroed(8);
        assert!(s.accel.is_none());
        assert!(s.analog.is_none());
        assert!(s.resistance.is_none());
    }

    #[test]
    fn zeroed_defaults() {
        let s = Sample::zeroed(4);
        assert_eq!(s.sample_num, 0);
        assert_eq!(s.end_byte, 0xC0);
        assert_eq!(s.aux_bytes, [0u8; 6]);
        assert_eq!(s.timestamp, 0.0);
    }

    // ── now() ────────────────────────────────────────────────────────────────

    #[test]
    fn now_is_reasonable_unix_timestamp() {
        let t = now();
        // Must be after 2020-01-01 and before 2100-01-01
        assert!(t > 1_577_836_800.0, "timestamp too old: {t}");
        assert!(t < 4_102_444_800.0, "timestamp too far in the future: {t}");
    }

    #[test]
    fn now_is_monotonically_non_decreasing() {
        let t1 = now();
        let t2 = now();
        assert!(t2 >= t1, "now() went backwards: {t1} > {t2}");
    }

    // ── StreamHandle ──────────────────────────────────────────────────────────

    fn make_stream_with_samples(samples: Vec<Sample>) -> StreamHandle {
        let (sample_tx, sample_rx) = mpsc::sync_channel(64);
        let (stop_tx, _stop_rx) = mpsc::sync_channel(1);
        for s in samples {
            sample_tx.send(s).unwrap();
        }
        StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) }
    }

    #[test]
    fn stream_handle_try_recv_empty() {
        let handle = make_stream_with_samples(vec![]);
        assert!(handle.try_recv().is_none());
    }

    #[test]
    fn stream_handle_try_recv_returns_samples() {
        let mut s = Sample::zeroed(4);
        s.sample_num = 42;
        let handle = make_stream_with_samples(vec![s]);
        let received = handle.try_recv().expect("should have a sample");
        assert_eq!(received.sample_num, 42);
    }

    #[test]
    fn stream_handle_recv_blocks_and_returns_none_when_done() {
        let handle = make_stream_with_samples(vec![]);
        // all senders are dropped, so recv() should return None immediately
        assert!(handle.recv().is_none());
    }

    #[test]
    fn stream_handle_into_iterator_yields_all_samples() {
        let samples: Vec<Sample> = (0..5u8)
            .map(|i| { let mut s = Sample::zeroed(4); s.sample_num = i; s })
            .collect();
        let handle = make_stream_with_samples(samples);
        let nums: Vec<u8> = handle.into_iter().map(|s| s.sample_num).collect();
        assert_eq!(nums, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn stream_handle_stop_sends_signal() {
        let (sample_tx, sample_rx) = mpsc::sync_channel::<Sample>(1);
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let handle = StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) };
        drop(sample_tx); // no more samples
        handle.stop();
        // stop signal received
        assert!(stop_rx.try_recv().is_ok());
    }

    #[test]
    fn stream_handle_drop_sends_stop_signal() {
        let (_, sample_rx) = mpsc::sync_channel::<Sample>(1);
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let handle = StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) };
        drop(handle); // Drop should fire the stop signal
        assert!(stop_rx.try_recv().is_ok(), "stop signal not sent on drop");
    }

    // ── Clone ─────────────────────────────────────────────────────────────────

    #[test]
    fn sample_clone_is_independent() {
        let mut s = Sample::zeroed(4);
        s.eeg[0] = 123.0;
        s.accel = Some([1.0, 2.0, 3.0]);
        let mut clone = s.clone();
        clone.eeg[0] = 999.0;
        assert_eq!(s.eeg[0], 123.0, "original should not be modified through clone");
    }
}
