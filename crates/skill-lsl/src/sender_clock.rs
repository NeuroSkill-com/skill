// SPDX-License-Identifier: GPL-3.0-only
//! How a sender's LSL clock stands against ours, measured here.
//!
//! An LSL outlet answers time queries (`LSL:timedata`, over UDP): the query carries our clock,
//! the answer adds the sender's at its arrival and at its departure, and from the four the
//! offset between the clocks and the time the exchange took on the wire follow, as in NTP.
//! Only exchanges that were quick are believed — a slow one says little about when the
//! sender's clock was read.
//!
//! rlsl's inlets measure this themselves (`PROC_CLOCKSYNC`), and up to 0.1.1 they do it
//! wrongly on a slow link: an answer is taken for the query that was sent last, not for the
//! one it answers. With 400 ms between a sender and us, the answer to one query arrives
//! while a later one is waited for, the exchange looks quick, and the offset is wrong by a
//! good part of the delay — a recording's time stamps ran 90 ms ahead while the link was
//! slow, and for a minute after. So the measurement is made here, where every answer is
//! matched to its own query by the number and the time it echoes, and the offset moves no
//! faster than clocks drift: a wrong measurement, should one get through, cannot move the
//! time stamps by more than a fraction of a millisecond.

use rlsl::stream_info::StreamInfo;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Queries per measurement, and how long each is given before the next is sent.
const PROBES: usize = 8;
const PROBE_EVERY: Duration = Duration::from_millis(60);
/// An exchange that took longer than this on the wire is not believed.
const MAX_ROUND_TRIP_S: f64 = 0.1;
/// The clocks are compared again this often.
const MEASURE_EVERY: Duration = Duration::from_secs(2);
/// The offset follows a new measurement by at most this much each time: 100 parts in a
/// million, more than two quartz clocks drift apart.
const MAX_STEP_S: f64 = 0.000_2;
/// A measurement this far from the offset in use is a clock that was set, not one that
/// drifted — if it says so this many times in a row.
const JUMP_S: f64 = 0.05;
const JUMPS_BEFORE_BELIEVED: u32 = 5;

/// One answered query: how long it took on the wire, and local clock minus sender's clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Exchange {
    pub round_trip_s: f64,
    pub offset_s: f64,
}

/// What an answer says, given the time it arrived (`t3`, our clock) and the queries that are
/// out (`sent`: number and the time each was sent). `None`: not an answer to any of them.
///
/// The answer is ` <number> <t0> <t1> <t2>`: the query's number and time as it sent them,
/// the sender's clock when the query arrived and when the answer left.
pub(crate) fn read_answer(answer: &str, sent: &[(u32, f64)], t3: f64) -> Option<Exchange> {
    let mut parts = answer.split_whitespace();
    let number: u32 = parts.next()?.parse().ok()?;
    let t0: f64 = parts.next()?.parse().ok()?;
    let t1: f64 = parts.next()?.parse().ok()?;
    let t2: f64 = parts.next()?.parse().ok()?;
    // the query this answers, by its number and by the time it carried — not whichever was
    // sent last
    sent.iter().find(|(n, at)| *n == number && (*at - t0).abs() < 1e-6)?;
    let round_trip_s = (t3 - t0) - (t2 - t1);
    // sender ahead of us by ((t1 - t0) + (t2 - t3)) / 2; what is added to its stamps is that, negated
    let offset_s = -((t1 - t0) + (t2 - t3)) / 2.0;
    (round_trip_s >= 0.0 && round_trip_s.is_finite()).then_some(Exchange { round_trip_s, offset_s })
}

/// The offset a set of exchanges gives: the median of the three quickest that were quick
/// enough. `None` when none was.
pub(crate) fn offset_of(mut exchanges: Vec<Exchange>) -> Option<f64> {
    exchanges.retain(|e| e.round_trip_s <= MAX_ROUND_TRIP_S);
    exchanges.sort_by(|a, b| a.round_trip_s.total_cmp(&b.round_trip_s));
    exchanges.truncate(3);
    let mut offsets: Vec<f64> = exchanges.iter().map(|e| e.offset_s).collect();
    offsets.sort_by(f64::total_cmp);
    offsets.get(offsets.len() / 2).copied()
}

/// The offset to use after a measurement: followed slowly, unless the sender's clock was set.
pub(crate) fn follow(in_use: f64, measured: f64, jumps: &mut u32) -> f64 {
    let apart = measured - in_use;
    if apart.abs() >= JUMP_S {
        *jumps += 1;
        if *jumps >= JUMPS_BEFORE_BELIEVED {
            *jumps = 0;
            return measured;
        }
        return in_use;
    }
    *jumps = 0;
    in_use + apart.clamp(-MAX_STEP_S, MAX_STEP_S)
}

fn time_service(info: &StreamInfo) -> Option<String> {
    let (v4, v6) = (info.v4address(), info.v6address());
    if !v4.is_empty() && info.v4service_port() > 0 {
        Some(format!("{v4}:{}", info.v4service_port()))
    } else if !v6.is_empty() && info.v6service_port() > 0 {
        Some(format!("[{v6}]:{}", info.v6service_port()))
    } else {
        None
    }
}

/// Ask the sender's time service a few times; what was answered in time.
fn measure(address: &str) -> Option<f64> {
    let socket = UdpSocket::bind(if address.starts_with('[') {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    })
    .ok()?;
    let mut sent: Vec<(u32, f64)> = Vec::with_capacity(PROBES);
    let mut exchanges = Vec::new();
    let mut buf = [0u8; 256];
    for number in 0..PROBES as u32 {
        let t0 = rlsl::clock::local_clock();
        // (as it will come back: the time is matched as text read again)
        let text = format!("{t0}");
        let t0: f64 = text.parse().ok()?;
        if socket
            .send_to(format!("LSL:timedata\r\n{number} {text}\r\n").as_bytes(), address)
            .is_err()
        {
            continue;
        }
        sent.push((number, t0));
        // whatever arrives until the next query is due: answers to this one or to earlier ones
        let until = Instant::now() + PROBE_EVERY;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
                break;
            }
            let Ok((n, _)) = socket.recv_from(&mut buf) else { break };
            let t3 = rlsl::clock::local_clock();
            if let Some(e) = std::str::from_utf8(&buf[..n])
                .ok()
                .and_then(|a| read_answer(a, &sent, t3))
            {
                exchanges.push(e);
            }
        }
    }
    offset_of(exchanges)
}

/// A sender's clock against ours, kept current for as long as this lives.
pub(crate) struct SenderClock {
    /// seconds to add to the sender's time stamps to have them on our LSL clock (bits of an f64)
    offset: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl SenderClock {
    /// Measure it once (`None`: the sender has no time service, or does not answer), and go on
    /// measuring in the background.
    pub(crate) fn of(info: &StreamInfo) -> Option<Self> {
        let address = time_service(info)?;
        // twice, if need be: the first query to a sender just started may go unanswered
        let first = measure(&address).or_else(|| measure(&address))?;
        let offset = Arc::new(AtomicU64::new(first.to_bits()));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopped) = (offset.clone(), stop.clone());
        let spawned = std::thread::Builder::new()
            .name("lsl-sender-clock".into())
            .spawn(move || {
                let mut jumps = 0;
                while !stopped.load(Ordering::Relaxed) {
                    // in short sleeps: the thread ends soon after its owner does
                    let until = Instant::now() + MEASURE_EVERY;
                    while Instant::now() < until && !stopped.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    if stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Some(measured) = measure(&address) {
                        let in_use = f64::from_bits(shared.load(Ordering::Relaxed));
                        shared.store(follow(in_use, measured, &mut jumps).to_bits(), Ordering::Relaxed);
                    }
                }
            });
        spawned.ok().map(|_| Self { offset, stop })
    }

    /// Seconds to add to the sender's time stamps to have them on our LSL clock.
    pub(crate) fn offset(&self) -> f64 {
        f64::from_bits(self.offset.load(Ordering::Relaxed))
    }
}

impl Drop for SenderClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_read_against_the_query_it_answers() {
        // the sender's clock is 100 s ahead; 2 ms each way, 1 ms at the sender
        let sent = [(0, 10.000), (1, 10.060), (2, 10.120)];
        let quick = read_answer(" 2 10.12 110.122 110.123", &sent, 10.125).unwrap();
        assert!(
            (quick.round_trip_s - 0.004).abs() < 1e-9 && (quick.offset_s + 100.0).abs() < 1e-9,
            "{quick:?}"
        );
        // The link is slow: the answer to the first query arrives while the third is waited
        // for. Read against its own query it is a slow exchange, and not believed —
        let late = read_answer(" 0 10 110.2 110.201", &sent, 10.401).unwrap();
        assert!((late.round_trip_s - 0.4).abs() < 1e-9, "{late:?}");
        assert_eq!(offset_of(vec![late]), None);
        // — where read against the query sent last it would have looked quick (0.28 s less)
        // and put the clocks 140 ms from where they are.
        // not an answer to anything that was asked
        assert_eq!(read_answer(" 7 10.12 110.122 110.123", &sent, 10.125), None);
        assert_eq!(read_answer(" 2 10.5 110.122 110.123", &sent, 10.125), None);
        assert_eq!(read_answer("nonsense", &sent, 10.125), None);
        // the quickest exchanges decide
        let e = |round_trip_s, offset_s| Exchange { round_trip_s, offset_s };
        assert_eq!(
            offset_of(vec![
                e(0.09, 5.0),
                e(0.002, 1.0),
                e(0.003, 1.1),
                e(0.004, 0.9),
                e(0.2, 9.0)
            ]),
            Some(1.0)
        );
    }

    #[test]
    fn the_offset_follows_slowly_and_a_clock_that_was_set_at_once() {
        let mut jumps = 0;
        // drift: followed, a fifth of a millisecond at most each time
        assert!((follow(1.0, 1.000_05, &mut jumps) - 1.000_05).abs() < 1e-12);
        assert!((follow(1.0, 1.003, &mut jumps) - 1.000_2).abs() < 1e-12);
        assert!((follow(1.0, 0.99, &mut jumps) - 0.999_8).abs() < 1e-12);
        // one measurement far off: not believed
        assert_eq!(follow(1.0, 1.2, &mut jumps), 1.0);
        assert_eq!(follow(1.0, 1.000_1, &mut jumps), 1.000_1);
        assert_eq!(jumps, 0);
        // the same far-off value time after time: the sender's clock was set
        let mut in_use = 1.0;
        for _ in 0..JUMPS_BEFORE_BELIEVED {
            in_use = follow(in_use, 3.0, &mut jumps);
        }
        assert_eq!(in_use, 3.0);
    }

    #[test]
    fn a_sender_in_this_program_is_on_the_same_clock() {
        let info = StreamInfo::new(
            &format!("SenderClock-{}", std::process::id()),
            "EEG",
            1,
            100.0,
            rlsl::types::ChannelFormat::Float32,
            "sender-clock-test",
        );
        let _outlet = rlsl::outlet::StreamOutlet::new(&info, 0, 360);
        let found = rlsl::resolver::resolve_by_property("name", &info.name(), 1, 5.0);
        let clock = SenderClock::of(found.first().expect("the stream resolves")).expect("its time service answers");
        assert!(clock.offset().abs() < 0.005, "{}", clock.offset());
    }
}
