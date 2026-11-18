// SPDX-License-Identifier: GPL-3.0-only
//! Local-network LSL stream → [`DeviceAdapter`].
//!
//! Frames leave here stamped in Unix time **on this machine's clock**, like
//! every other device's — and like the labels and markers they are compared
//! with. LSL stamps are seconds on the *sender's* clock, so each stream is
//! mapped, by the best means it offers:
//!
//! 1. **LSL clock synchronisation.** The outlet's time service says how far
//!    its clock is from ours (re-measured every few seconds, so drift is
//!    followed), and our own LSL clock is tied to our Unix clock. A sample
//!    stamped by a source on another computer — whose wall clock may be tens
//!    of milliseconds, or hours, from ours — comes out at the moment it was
//!    taken, as this machine tells time.
//! 2. **A declared wall-clock reference** (`<clock_reference>` in the
//!    description, as the `nesdk-lsl` bridge writes), when the time service
//!    does not answer: exact on the sender's wall clock.
//! 3. **Arrival**: the first samples are anchored to when they got here.
//!
//! Markers go the same way, so a marker and the EEG sample it belongs to
//! carry the same time whichever computers they came from. An accelerometer
//! stream published beside an `…-EEG` stream by the same source is recorded
//! with it.
//!
//! While a session runs, every LSL marker stream on the network (type
//! `Markers`, irregular rate — what experiment software publishes its events
//! on) is followed too, including ones that appear later, and each marker is
//! delivered as a [`DeviceEvent::Marker`].

use async_trait::async_trait;
use rlsl::resolver;
use rlsl::stream_info::StreamInfo;
use tokio::sync::mpsc;

use skill_devices::session::{
    DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame, MarkerFrame,
};

/// The pause between searches of the network for marker streams that were
/// not there before. Short, with a short search: what a marker stream sends
/// before it is found is lost — LSL does not replay.
const MARKER_RESCAN: std::time::Duration = std::time::Duration::from_millis(500);
/// Seconds of a newly found marker stream's past that are asked for.
const MARKER_REPLAY_SECS: f64 = 10.0;
/// How long each search listens for answers.
const MARKER_SEARCH_SECS: f64 = 0.5;

/// Unix time of markers from one stream.
///
/// A marker is stamped by its sender, on the sender's clock; what is known
/// here is when it arrived. The smallest difference seen so far between the
/// two is the clock offset plus the fastest trip any marker has made, so the
/// first marker is placed at its arrival and later ones at their stamps,
/// free of the delay each happened to have.
#[derive(Debug, Default)]
pub(crate) struct MarkerClock {
    offset: Option<f64>,
}

impl MarkerClock {
    pub(crate) fn unix_time(&mut self, stamp: f64, arrived_unix: f64) -> f64 {
        // No usable stamp: the arrival is all there is.
        if !stamp.is_finite() || stamp <= 0.0 {
            return arrived_unix;
        }
        let seen = arrived_unix - stamp;
        let offset = self.offset.map_or(seen, |o| o.min(seen));
        self.offset = Some(offset);
        stamp + offset
    }
}

/// A marker sample as text: the channels of a string stream joined with `;`,
/// numbers without a needless `.0`.
pub(crate) fn marker_text_from_numbers(values: &[f64]) -> String {
    values
        .iter()
        .map(|v| {
            if v.fract() == 0.0 && v.abs() < 1e15 {
                format!("{}", *v as i64)
            } else {
                format!("{v}")
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Whether a stream is an event-marker stream.
fn is_marker_stream(info: &StreamInfo) -> bool {
    info.type_().eq_ignore_ascii_case("markers") && info.nominal_srate() == 0.0 && info.channel_count() >= 1
}

/// Follow every marker stream on the network for as long as `tx` has a
/// receiver, forwarding each marker as a [`DeviceEvent::Marker`].
fn spawn_marker_watcher(tx: mpsc::Sender<DeviceEvent>) {
    let following = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::<String>::new()));
    let spawned = std::thread::Builder::new().name("lsl-markers".into()).spawn(move || {
        while !tx.is_closed() {
            for info in resolver::resolve_all(MARKER_SEARCH_SECS)
                .into_iter()
                .filter(is_marker_stream)
            {
                let uid = info.uid();
                if !following.lock().is_ok_and(|mut f| f.insert(uid.clone())) {
                    continue;
                }
                let (tx, following) = (tx.clone(), following.clone());
                let name = info.name();
                let started = std::thread::Builder::new()
                    .name(format!("lsl-marker-{name}"))
                    .spawn(move || {
                        follow_marker_stream(&info, &tx);
                        // Gone (or never opened): let a stream that comes
                        // back be followed again.
                        if let Ok(mut f) = following.lock() {
                            f.remove(&uid);
                        }
                    });
                if started.is_err() {
                    eprintln!("[lsl] could not start a thread for marker stream '{name}'");
                }
            }
            // Sleep in short steps so the thread ends soon after the session.
            let until = std::time::Instant::now() + MARKER_RESCAN;
            while std::time::Instant::now() < until && !tx.is_closed() {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });
    if spawned.is_err() {
        eprintln!("[lsl] could not start the marker watcher; markers will not be recorded");
    }
}

/// A marker's time, or its arrival when that time is not one: a marker cannot
/// come from the future, nor from before what a sender replays. Such a stamp is
/// not on the sender's clock at all (a program that numbers its markers instead
/// of timing them); the marker happened when it got here.
pub(crate) fn plausible_marker_time(timestamp_s: f64, now: f64, source: &str, warned: &mut bool) -> f64 {
    if timestamp_s <= now + 1.0 && timestamp_s >= now - 60.0 {
        return timestamp_s;
    }
    if !*warned {
        eprintln!(
            "[lsl] marker stream '{source}': its time stamps are not times ({:+.0} s from now); its markers are placed at their arrival",
            timestamp_s - now
        );
        *warned = true;
    }
    now
}

/// Pull markers from one stream until it goes away or the session ends.
fn follow_marker_stream(info: &StreamInfo, tx: &mpsc::Sender<DeviceEvent>) {
    let source = info.name();
    let inlet = rlsl::inlet::StreamInlet::new(info, 360, 0, true);
    // What the stream sent before it was found here (it is looked for about once a second):
    // an rlsl sender keeps its last seconds for exactly this and sends them first; any other
    // sender ignores the request, and those markers are lost as before.
    inlet.set_replay(MARKER_REPLAY_SECS);
    if let Err(e) = inlet.open_stream(5.0) {
        eprintln!("[lsl] marker stream '{source}' found but not opened: {e}");
        return;
    }
    eprintln!("[lsl] following marker stream '{source}'");
    let text_stream = info.channel_format() == rlsl::types::ChannelFormat::String;
    let mut numbers = vec![0.0f64; info.channel_count() as usize];
    // The marker's own instant on our clock when the stream's time service
    // answers; else its arrival, less the transport delay as far as known.
    let mut synced = synchronise_clock(&inlet, info, false).map(|sender| (sender, WallClock::new()));
    let mut clock = MarkerClock::default();
    let mut warned = false;
    while !tx.is_closed() {
        let pulled = if text_stream {
            inlet
                .pull_sample_str(0.2)
                .map(|(values, stamp)| (values.join(";"), stamp))
        } else {
            inlet
                .pull_sample_d(&mut numbers, 0.2)
                .map(|stamp| (marker_text_from_numbers(&numbers), stamp))
        };
        // an error: the stream is gone
        let Ok((text, stamp)) = pulled else { return };
        // a timeout: no stamp
        if stamp == 0.0 {
            continue;
        }
        let now = unix_now();
        let frame = MarkerFrame {
            timestamp_s: plausible_marker_time(
                match synced.as_mut() {
                    Some((sender, wall)) => stamp + sender.offset() + wall.offset(),
                    None => clock.unix_time(stamp, now),
                },
                now,
                &source,
                &mut warned,
            ),
            text,
            source: source.clone(),
        };
        if tx.blocking_send(DeviceEvent::Marker(frame)).is_err() {
            return;
        }
    }
}

/// Standard gravity in mm/s², to turn an accelerometer stream in mm/s² into g.
const G_MM_S2: f64 = 9806.65;

/// Whether to ask a stream's outlet for a compressed feed.
///
/// Compression saves network, not time: it is asked for when the source is on
/// another computer and not for one on this machine. `SKILL_LSL_COMPRESSION`
/// overrides the choice (`1` always, `0` never). Asking costs nothing where it
/// cannot be given — an outlet that does not know the request (liblsl, an
/// older rlsl) sends the plain protocol.
fn wants_compression(info: &StreamInfo) -> bool {
    match std::env::var("SKILL_LSL_COMPRESSION").ok().as_deref() {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => {
            let host = info.hostname();
            // a fresh stream description carries this machine's own name
            let here = StreamInfo::new("", "", 0, 0.0, rlsl::types::ChannelFormat::Float32, "").hostname();
            !host.is_empty() && !here.is_empty() && !host.eq_ignore_ascii_case(&here)
        }
    }
}

/// The LSL clock on the Unix timeline, kept current: the two run at slightly
/// different rates (54 ms apart after an hour on the machine this was
/// measured on), so the difference is re-measured every few seconds and
/// followed smoothly. See [`rlsl::clock::UnixOffset`].
pub(crate) use rlsl::clock::UnixOffset as WallClock;

/// Put an inlet's time stamps on this machine's LSL clock: `true` when the
/// outlet's time service answered and the inlet now keeps the offset itself
/// (re-measuring it in the background), `false` when it did not — the stamps
/// then stay on the sender's clock.
fn synchronise_clock(
    inlet: &rlsl::inlet::StreamInlet,
    info: &StreamInfo,
    regular_rate: bool,
) -> Option<crate::sender_clock::SenderClock> {
    // The sender's clock against ours is measured here, not by the inlet (see
    // `sender_clock` for why), and added to the stamps where they are used.
    let sender = crate::sender_clock::SenderClock::of(info)?;
    inlet.set_clock_sync(false);
    // Regular streams are smoothed; markers keep their own instants.
    let smoothing = if regular_rate { rlsl::types::PROC_DEJITTER } else { 0 };
    inlet.set_postprocessing(smoothing | rlsl::types::PROC_MONOTONIZE | rlsl::types::PROC_THREADSAFE);
    Some(sender)
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Maps a stream's LSL time stamps to Unix time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ClockMap {
    /// Nothing seen yet: the first chunk decides.
    Pending,
    /// `unix = stamp + offset`.
    Offset {
        offset: f64,
        /// From the stream's own `<clock_reference>` (exact acquisition time)
        /// rather than from when its samples arrived here.
        exact: bool,
    },
}

impl ClockMap {
    /// The mapping a stream declares: a stamp `lsl_time` was taken at Unix
    /// time `unix_time`.
    pub(crate) fn from_reference(lsl_time: f64, unix_time: f64) -> Self {
        Self::Offset {
            offset: unix_time - lsl_time,
            exact: true,
        }
    }

    /// Unix time of a stamp. `newest` is the newest stamp of the chunk being
    /// mapped and `now` the Unix time it arrived at: the first chunk anchors
    /// an undeclared clock so that its newest sample is "now".
    pub(crate) fn unix_time(&mut self, stamp: f64, newest: f64, now: f64) -> f64 {
        if let Self::Pending = self {
            *self = Self::Offset {
                offset: now - newest,
                exact: false,
            };
        }
        match self {
            Self::Offset { offset, .. } => stamp + *offset,
            Self::Pending => stamp,
        }
    }
}

/// `<clock_reference><lsl_time>…</lsl_time><unix_time>…</unix_time></clock_reference>`
/// in a full-info XML document.
pub(crate) fn parse_clock_reference(xml: &str) -> Option<ClockMap> {
    let start = xml.find("<clock_reference>")?;
    let section = &xml[start..start + xml[start..].find("</clock_reference>")?];
    let tag = |name: &str| -> Option<f64> {
        let open = format!("<{name}>");
        let a = section.find(&open)? + open.len();
        let b = a + section[a..].find(&format!("</{name}>"))?;
        section[a..b].trim().parse().ok()
    };
    Some(ClockMap::from_reference(tag("lsl_time")?, tag("unix_time")?))
}

/// What the adapter has seen of the stream, for the session trace.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LslStreamStats {
    /// EEG samples received.
    pub samples: u64,
    /// Breaks in the time stamps (more than 1.5 sample periods apart).
    pub gaps: u64,
    /// Samples the breaks stand for, from the nominal rate.
    pub missing_samples: u64,
    /// The time stamps are exact acquisition times: by LSL clock
    /// synchronisation, or from the stream's declared wall-clock reference.
    pub exact_clock: bool,
    /// They are on this machine's clock, by LSL clock synchronisation (the
    /// outlet's time service answered).
    pub lsl_synced: bool,
    /// The outlet sends the EEG feed compressed (asked for when the source is
    /// on another computer, and given by sources that can).
    pub compressed: bool,
    /// Times the stream went silent and was found again as another outlet
    /// (the program that sends it was restarted).
    pub restarts: u64,
}

/// How long a stream with a regular rate may send nothing before it is looked
/// for again on the network.
const SILENCE_BEFORE_SEARCH: std::time::Duration = std::time::Duration::from_millis(1500);

/// How much of what a restarted sender has already sent is asked for when it is
/// gone over to: a sender that keeps its last seconds gives them, any other
/// ignores the request.
const RESTART_REPLAY_SECS: f64 = 3.0;

/// The marker source of a stream that went away and came back.
pub const RESTART_MARKER_SOURCE: &str = "lsl";

/// The outlet that took the place of `lost`: the same stream — source id (or,
/// for a stream without one, the computer it comes from), name, type, channel
/// count and format — from another outlet. `None` while there is none, also
/// when the outlet that fell silent is still the one on the network.
///
/// An LSL inlet is meant to do this by itself. rlsl up to 0.1.0 looks for the
/// lost outlet's own id, which a restarted program never has again, so a
/// session waited for ever beside a stream that was back; this is the way
/// around it, and it stays harmless once the inlet recovers on its own (data
/// is then flowing, and nobody looks).
fn find_restarted(lost: &StreamInfo) -> Option<StreamInfo> {
    let source_id = lost.source_id();
    // By name as well as by source: a device's streams share its source id (its
    // EEG and its accelerometer), and the search ends with the first answer —
    // the accelerometer's, as often as not, and then nothing was found.
    let name = lost.name();
    let found = if source_id.is_empty() {
        resolver::resolve_query(&format!("name='{name}'"), 1, 1.0)
    } else if source_id.contains('\'') || name.contains('\'') {
        resolver::resolve_by_property("source_id", &source_id, 0, 1.0)
    } else {
        resolver::resolve_query(&format!("source_id='{source_id}' and name='{name}'"), 1, 1.0)
    };
    if found.iter().any(|s| s.uid() == lost.uid()) {
        return None;
    }
    found.into_iter().find(|s| {
        s.source_id() == source_id
            && (!source_id.is_empty() || s.hostname() == lost.hostname())
            && s.name() == lost.name()
            && s.type_() == lost.type_()
            && s.channel_count() == lost.channel_count()
            && s.channel_format() == lost.channel_format()
    })
}

/// Discover LSL EEG/EXG streams on the local network.
pub fn resolve_eeg_streams(timeout_secs: f64) -> Vec<StreamInfo> {
    resolver::resolve_all(timeout_secs)
        .into_iter()
        .filter(|s| {
            let t = s.type_().to_lowercase();
            t == "eeg" || t == "exg" || t == "biosignal"
        })
        .collect()
}

/// Resolve a single LSL stream by name.  Returns as soon as a match is found
/// (typically < 500 ms for local streams) rather than waiting the full timeout.
pub fn resolve_stream_by_name(name: &str, timeout_secs: f64) -> Option<StreamInfo> {
    let query = format!("name='{name}'");
    let mut results = resolver::resolve_query(&query, 1, timeout_secs);
    if results.is_empty() {
        None
    } else {
        Some(results.swap_remove(0))
    }
}

/// Lightweight description of a discovered LSL stream for UI display.
#[derive(Clone)]
pub struct LslStreamInfo {
    pub name: String,
    pub stream_type: String,
    pub channel_count: usize,
    pub sample_rate: f64,
    pub source_id: String,
    pub hostname: String,
    pub info: StreamInfo,
}

/// Resolve and return display-friendly stream info.
pub fn discover_streams(timeout_secs: f64) -> Vec<LslStreamInfo> {
    resolver::resolve_all(timeout_secs)
        .into_iter()
        .map(|s| LslStreamInfo {
            name: s.name().to_string(),
            stream_type: s.type_().to_string(),
            channel_count: s.channel_count() as usize,
            sample_rate: s.nominal_srate(),
            source_id: s.source_id().to_string(),
            hostname: s.hostname().to_string(),
            info: s,
        })
        .collect()
}

/// DeviceAdapter that pulls from a local LSL stream.
pub struct LslAdapter {
    rx: mpsc::Receiver<DeviceEvent>,
    desc: DeviceDescriptor,
    stats: std::sync::Arc<std::sync::Mutex<LslStreamStats>>,
    /// Microvolts per count, for a stream of integers.
    eeg_scale_uv: Option<f64>,
    _shutdown: mpsc::Sender<()>,
}

impl LslAdapter {
    /// Connect to an LSL stream and return an adapter that produces
    /// [`DeviceEvent`]s.  Returns an error if the TCP data connection
    /// cannot be established (outlet gone, firewall, timeout).
    pub fn connect(info: &StreamInfo) -> Result<Self, String> {
        let channel_count = info.channel_count() as usize;
        let sample_rate = info.nominal_srate();
        let name = info.name().to_string();
        let stream_type = info.type_().to_string();
        let source_id = info.source_id().to_string();
        let hostname = info.hostname().to_string();

        // Read channel labels from the LSL stream's XML description.
        //
        // Path 1 (in-process): when the caller passes the outlet's own
        // StreamInfo (e.g. unit tests), `info.desc()` already contains
        // the full `<channels>` XML — read labels directly.
        //
        // Path 2 (network-resolved): `resolve_query` returns a minimal
        // StreamInfo whose desc is `<desc></desc>`.  `rlsl` 0.0.4's
        // `get_fullinfo()` is a no-op stub, so we must fetch the full
        // XML ourselves via a TCP `LSL:fullinfo` request to the outlet's
        // data port and parse the `<label>` tags from the response.
        let mut clock = read_clock_reference_from_desc(info).unwrap_or(ClockMap::Pending);
        let mut description = info.desc().to_xml();
        let channel_names: Vec<String> = {
            let mut names = read_labels_from_desc(info, channel_count);

            if names.is_empty() {
                // desc was empty (network-resolved) — fetch via TCP.
                let xml = fetch_fullinfo_xml(info).unwrap_or_default();
                names = parse_labels_from_xml(&xml, channel_count);
                description.clone_from(&xml);
                if let Some(c) = parse_clock_reference(&xml) {
                    clock = c;
                }
            }

            // Pad / truncate to exact channel count.
            while names.len() < channel_count {
                names.push(format!("Ch{}", names.len() + 1));
            }
            names.truncate(channel_count);
            names
        };

        // The pipeline works in microvolts: a stream that declares another
        // unit of voltage is converted. A stream of integers is whole counts
        // of its unit, which a recording can store as they are.
        let to_uv = declared_unit(&description)
            .and_then(|u| unit_in_microvolts(&u))
            .unwrap_or(1.0);
        let is_integer = matches!(
            info.channel_format(),
            rlsl::types::ChannelFormat::Int8
                | rlsl::types::ChannelFormat::Int16
                | rlsl::types::ChannelFormat::Int32
                | rlsl::types::ChannelFormat::Int64
        );
        let eeg_scale_uv = is_integer.then_some(to_uv);

        // The accelerometer a source publishes beside its EEG stream
        // (`<name>-EEG` / `<name>-Accelerometer`, same source id).
        let accel_info = find_accelerometer_sibling(info);

        let desc = DeviceDescriptor {
            kind: "lsl",
            caps: if accel_info.is_some() {
                DeviceCaps::EEG | DeviceCaps::IMU
            } else {
                DeviceCaps::EEG
            },
            eeg_channels: channel_count,
            eeg_sample_rate: sample_rate,
            channel_names,
            // DSP pipeline processes up to EEG_CHANNELS; all channels stored in CSV
            pipeline_channels: channel_count.min(skill_constants::EEG_CHANNELS),
            ppg_channel_names: Vec::new(),
            imu_channel_names: if accel_info.is_some() {
                vec!["AccelX".into(), "AccelY".into(), "AccelZ".into()]
            } else {
                Vec::new()
            },
            fnirs_channel_names: Vec::new(),
        };

        // Room for ~4 s at 500 Hz: the session loop also does DSP and a stall
        // there should be absorbed here before it backs up into the inlet.
        let (tx, rx) = mpsc::channel(2048);
        let stats = std::sync::Arc::new(std::sync::Mutex::new(LslStreamStats {
            exact_clock: matches!(clock, ClockMap::Offset { exact: true, .. }),
            ..Default::default()
        }));
        let thread_stats = stats.clone();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        spawn_marker_watcher(tx.clone());
        let info_clone = info.clone();
        let thread_channel_names = desc.channel_names.clone();
        let name_for_timeout = name.clone();

        // Channel for the inlet thread to report open_stream result back.
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        std::thread::Builder::new()
            .name(format!("lsl-inlet-{name}"))
            .spawn(move || {
                let mut current = info_clone.clone();
                let mut inlet = rlsl::inlet::StreamInlet::new(&info_clone, 360, 0, true);
                inlet.set_compression(wants_compression(&info_clone));
                let mut clock = clock;
                let mut wall: Option<WallClock> = None;
                let mut sender = synchronise_clock(&inlet, &info_clone, sample_rate > 0.0);
                if sender.is_some() {
                    wall = Some(WallClock::new());
                    // On our LSL clock now; that clock to Unix is a constant.
                    clock = ClockMap::Offset {
                        offset: rlsl::clock::unix_offset_now()
                            + sender.as_ref().map_or(0.0, crate::sender_clock::SenderClock::offset),
                        exact: true,
                    };
                    if let Ok(mut s) = thread_stats.lock() {
                        s.exact_clock = true;
                        s.lsl_synced = true;
                    }
                } else {
                    // No time service: smoothed and monotonic, but left on the
                    // sender's clock — the declared reference, or the arrival
                    // anchor, is defined on that clock.
                    inlet.set_postprocessing(
                        rlsl::types::PROC_DEJITTER | rlsl::types::PROC_MONOTONIZE | rlsl::types::PROC_THREADSAFE,
                    );
                }

                // Open the TCP data connection.  Signal the result back so
                // connect() can return an error to the caller rather than
                // silently entering a broken session.
                if let Err(e) = inlet.open_stream(10.0) {
                    let _ = ready_tx.send(Err(format!("LSL stream '{name}' found but TCP connection failed: {e}")));
                    return;
                }
                if let Ok(mut s) = thread_stats.lock() {
                    s.compressed = inlet.is_compressed();
                }
                let _ = ready_tx.send(Ok(()));

                eprintln!(
                    "[lsl] connected to '{}' — {} ch @ {} Hz{} | channels: [{}]",
                    name,
                    channel_count,
                    sample_rate,
                    if inlet.is_compressed() { ", compressed feed" } else { "" },
                    thread_channel_names.join(", "),
                );

                // Build a descriptive hardware_version from LSL metadata:
                // "EEG · 32ch · 256 Hz" or "EEG · 32ch · 256 Hz @ hostname"
                let hw_version = if hostname.is_empty() {
                    format!("{stream_type} · {channel_count}ch · {sample_rate} Hz")
                } else {
                    format!("{stream_type} · {channel_count}ch · {sample_rate} Hz @ {hostname}")
                };

                let _ = tx.blocking_send(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: source_id,
                    serial_number: None,
                    firmware_version: None,
                    hardware_version: Some(hw_version),
                    bootloader_version: None,
                    mac_address: None,
                    headset_preset: None,
                }));

                // The accelerometer stream, when there is one. A failure to
                // open it costs the accelerometer, not the session.
                let open_accelerometer = |a: StreamInfo| {
                    let to_g = if accelerometer_unit_is_mm_s2(&a) {
                        1.0 / G_MM_S2
                    } else {
                        1.0
                    };
                    let inlet = rlsl::inlet::StreamInlet::new(&a, 360, 0, true);
                    // Its readings on our clock too, so that they sit on
                    // the EEG's timeline and not at whenever they got here.
                    let synced = synchronise_clock(&inlet, &a, true);
                    inlet.open_stream(5.0).ok().map(|()| (inlet, to_g, synced))
                };
                let mut accel = accel_info.and_then(open_accelerometer);

                // Pull samples in batches to reduce per-sample overhead.
                let period = if sample_rate > 0.0 { 1.0 / sample_rate } else { 0.0 };
                let mut last_stamp: Option<f64> = None;
                let mut last_data = std::time::Instant::now();
                let mut searched = std::time::Instant::now();
                // the sender's own stamp of the newest sample, to see its clock change
                let mut last_raw: Option<f64> = None;
                // the outlet that took the place of the one we were on, to be gone over to
                let mut restarted: Option<StreamInfo> = None;
                // the inlet went over to another outlet by itself, and we have not yet: what
                // it delivers meanwhile is on a clock we have not measured
                let mut behind = false;
                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    // A stream that should be sending and is not: its sender may
                    // have been restarted, and is then another outlet.
                    if period > 0.0
                        && restarted.is_none()
                        && (behind || last_data.elapsed() >= SILENCE_BEFORE_SEARCH)
                        && searched.elapsed() >= std::time::Duration::from_secs(1)
                    {
                        searched = std::time::Instant::now();
                        restarted = find_restarted(&current);
                    }
                    // Go over to it: a new inlet, its clock measured anew — nothing of the
                    // old sender's clock is carried along.
                    if let Some(found) = restarted.take() {
                        let fresh = rlsl::inlet::StreamInlet::new(&found, 360, 0, true);
                        fresh.set_compression(wants_compression(&found));
                        // what the new sender has sent since it started, if it keeps it:
                        // the moments it took us to get here
                        fresh.set_replay(RESTART_REPLAY_SECS);
                        // On the clock the recording is on, or not at all: a
                        // stream whose stamps were being put on this machine's
                        // clock is not taken back on its sender's.
                        let fresh_sender = synchronise_clock(&fresh, &found, true);
                        let synced = fresh_sender.is_some();
                        if !synced {
                            fresh.set_postprocessing(
                                rlsl::types::PROC_DEJITTER
                                    | rlsl::types::PROC_MONOTONIZE
                                    | rlsl::types::PROC_THREADSAFE,
                            );
                        }
                        if (synced || wall.is_none()) && fresh.open_stream(5.0).is_ok() {
                            if !synced {
                                // the new sender's clock: its declared reference, or
                                // the arrival of its first samples
                                clock = fetch_fullinfo_xml(&found)
                                    .and_then(|xml| parse_clock_reference(&xml))
                                    .unwrap_or(ClockMap::Pending);
                            }
                            if synced && wall.is_none() {
                                // this sender has a time service, the first had not
                                wall = Some(WallClock::new());
                                if let Ok(mut s) = thread_stats.lock() {
                                    s.exact_clock = true;
                                    s.lsl_synced = true;
                                }
                            }
                            eprintln!("[lsl] '{name}' was restarted by its sender; connected to it again");
                            accel = find_accelerometer_sibling(&found).and_then(open_accelerometer);
                            inlet = fresh;
                            sender = fresh_sender;
                            current = found;
                            behind = false;
                            last_raw = None;
                            if !synced {
                                // another clock, not measured against ours: the break
                                // cannot be counted in its stamps
                                last_stamp = None;
                            }
                            last_data = std::time::Instant::now();
                            if let Ok(mut s) = thread_stats.lock() {
                                s.restarts += 1;
                                s.compressed = inlet.is_compressed();
                            }
                            let _ = tx.blocking_send(DeviceEvent::Marker(MarkerFrame {
                                timestamp_s: unix_now(),
                                text: "source restarted".into(),
                                source: RESTART_MARKER_SOURCE.into(),
                            }));
                        }
                    }

                    if let Some((accel, to_g, accel_synced)) = accel.as_ref() {
                        if let Ok((stamps, values)) = accel.pull_chunk_d(64, 0.0) {
                            let to_unix = wall
                                .as_mut()
                                .zip(accel_synced.as_ref())
                                .map(|(wall, sender)| wall.offset() + sender.offset());
                            for (row, stamp) in values.as_chunks::<3>().0.iter().zip(&stamps) {
                                let g = |v: f64| (v * to_g) as f32;
                                let frame = ImuFrame {
                                    timestamp_s: to_unix.map(|offset| stamp + offset),
                                    accel: [g(row[0]), g(row[1]), g(row[2])],
                                    gyro: None,
                                    mag: None,
                                };
                                if tx.blocking_send(DeviceEvent::Imu(frame)).is_err() {
                                    return;
                                }
                            }
                        }
                    }

                    let Ok((timestamps, data)) = inlet.pull_chunk_d(256, 0.1) else {
                        continue;
                    };
                    let (Some(&oldest), Some(&newest)) = (timestamps.first(), timestamps.last()) else {
                        continue;
                    };
                    // Data again after a silence, or stamps that are suddenly from
                    // another clock, and nobody above went looking: the inlet found its
                    // stream again by itself. If that is another outlet — the sender was
                    // restarted — its clock is not the one we measured: what the inlet
                    // delivers is left aside, and the stream is gone over to as above
                    // (which asks the sender for what it has sent meanwhile).
                    if period > 0.0 {
                        let silent = last_data.elapsed();
                        let step = last_raw.map(|raw| oldest - raw);
                        let another_clock = step.is_some_and(|s| s < -0.001 || s > silent.as_secs_f64() + 1.0);
                        if behind || silent >= std::time::Duration::from_millis(300) || another_clock {
                            restarted = find_restarted(&current);
                            behind = behind || restarted.is_some();
                        }
                        if behind {
                            continue;
                        }
                    }
                    last_raw = Some(newest);
                    last_data = std::time::Instant::now();
                    let now = unix_now();
                    // On our LSL clock: keep its place on the Unix timeline
                    // current (one value for the chunk; it moves by
                    // microseconds between chunks).
                    if let Some(wall) = wall.as_mut() {
                        clock = ClockMap::Offset {
                            // the sender's clock to ours, and ours to Unix time
                            offset: wall.offset()
                                + sender.as_ref().map_or(0.0, crate::sender_clock::SenderClock::offset),
                            exact: true,
                        };
                    }

                    // Breaks in the stream: what the sender (or the network
                    // buffers) dropped shows up as a jump in the stamps.
                    let (mut gaps, mut missing) = (0u64, 0u64);
                    if period > 0.0 {
                        // (on our clock, where the sender's is measured: a break is
                        // then as long as it was, also across a sender that was
                        // restarted and counts its time from somewhere else)
                        let to_ours = sender.as_ref().map_or(0.0, crate::sender_clock::SenderClock::offset);
                        for ts in timestamps.iter().map(|ts| ts + to_ours) {
                            if let Some(prev) = last_stamp {
                                let steps = (ts - prev) / period;
                                if steps > 1.5 {
                                    gaps += 1;
                                    missing += (steps.round() as u64).saturating_sub(1);
                                }
                            }
                            last_stamp = Some(ts);
                        }
                    }
                    if let Ok(mut s) = thread_stats.lock() {
                        s.samples += timestamps.len() as u64;
                        s.gaps += gaps;
                        s.missing_samples += missing;
                    }

                    let n_ch = channel_count;
                    for (i, &ts) in timestamps.iter().enumerate() {
                        let offset = i * n_ch;
                        let mut channels = data[offset..offset + n_ch].to_vec();
                        if to_uv != 1.0 {
                            for v in &mut channels {
                                *v *= to_uv;
                            }
                        }
                        if tx
                            .blocking_send(DeviceEvent::Eeg(EegFrame {
                                channels,
                                timestamp_s: clock.unix_time(ts, newest, now),
                            }))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .expect("failed to spawn LSL inlet thread");

        // Wait for the inlet thread to report open_stream result.
        // Timeout matches open_stream's 10 s + 2 s grace.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(12)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(format!("LSL stream '{}' connection timed out", name_for_timeout)),
        }

        Ok(Self {
            rx,
            desc,
            stats,
            eeg_scale_uv,
            _shutdown: shutdown_tx,
        })
    }

    /// Samples received and breaks seen in the stream so far.
    pub fn stats(&self) -> LslStreamStats {
        self.stats.lock().map(|s| *s).unwrap_or_default()
    }

    /// Backwards-compatible constructor that panics on failure.
    /// Prefer [`connect`] for new code.
    pub fn new(info: &StreamInfo) -> Self {
        Self::connect(info).expect("LSL connect failed")
    }
}

// ── Channel label helpers ────────────────────────────────────────────────────

/// Read channel labels from `info.desc()` (works when the StreamInfo was
/// created in this process, e.g. tests sharing the outlet's StreamInfo).
fn read_labels_from_desc(info: &StreamInfo, channel_count: usize) -> Vec<String> {
    let desc = info.desc();
    let channels_node = desc.child("channels");
    if channels_node.is_empty() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(channel_count);
    let mut ch = channels_node.child("channel");
    while !ch.is_empty() {
        let label = ch.child_value("label");
        names.push(if label.is_empty() {
            format!("Ch{}", names.len() + 1)
        } else {
            label
        });
        ch = ch.next_sibling_named("channel");
    }
    names
}

/// The unit a stream description declares for its (first) channel.
fn declared_unit(xml: &str) -> Option<String> {
    let from = xml.find("<unit>")? + 6;
    Some(xml[from..from + xml[from..].find("</unit>")?].trim().to_string())
}

/// How many microvolts one of `unit` is, for the units of voltage a stream
/// may declare. `None` for anything else, which is taken as it comes.
fn unit_in_microvolts(unit: &str) -> Option<f64> {
    // "mV" and "MV" must not meet: only the spelled-out names ignore case.
    match unit {
        "uV" | "µV" | "μV" => return Some(1.0),
        "nV" => return Some(1e-3),
        "mV" => return Some(1e3),
        "V" => return Some(1e6),
        _ => {}
    }
    match unit.to_ascii_lowercase().as_str() {
        "microvolts" | "microvolt" => Some(1.0),
        "nanovolts" | "nanovolt" => Some(1e-3),
        "millivolts" | "millivolt" => Some(1e3),
        "volts" | "volt" => Some(1e6),
        _ => None,
    }
}

/// The wall-clock reference in `info.desc()` (in-process StreamInfo).
fn read_clock_reference_from_desc(info: &StreamInfo) -> Option<ClockMap> {
    let node = info.desc().child("clock_reference");
    if node.is_empty() {
        return None;
    }
    let value = |name: &str| node.child_value(name).trim().parse::<f64>().ok();
    Some(ClockMap::from_reference(value("lsl_time")?, value("unix_time")?))
}

/// The accelerometer stream published beside `info`: for an EEG stream
/// called `<name>-EEG`, the stream `<name>-Accelerometer` of the same source
/// with three channels.
fn find_accelerometer_sibling(info: &StreamInfo) -> Option<StreamInfo> {
    let name = info.name();
    let prefix = name.strip_suffix("-EEG")?;
    let source_id = info.source_id();
    resolver::resolve_query(&format!("name='{prefix}-Accelerometer'"), 1, 1.0)
        .into_iter()
        .find(|s| s.channel_count() == 3 && s.source_id() == source_id)
}

/// Whether an accelerometer stream reports mm/s² (as the NeuroElectrics
/// bridge does) rather than g.
fn accelerometer_unit_is_mm_s2(info: &StreamInfo) -> bool {
    let unit_of = |xml: &str| {
        let a = xml.find("<unit>")? + 6;
        Some(xml[a..a + xml[a..].find("</unit>")?].trim().to_ascii_lowercase())
    };
    let unit = unit_of(&info.desc().to_xml()).or_else(|| unit_of(&fetch_fullinfo_xml(info)?));
    unit.is_some_and(|u| u.starts_with("mm/s"))
}

/// Fetch the stream's full-info XML by sending `LSL:fullinfo` over TCP to the
/// outlet's data port.  `rlsl` 0.0.4's `get_fullinfo()` is a stub, so we must
/// implement the TCP request ourselves.
fn fetch_fullinfo_xml(info: &StreamInfo) -> Option<String> {
    use std::io::{BufRead, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    // Determine the outlet's TCP address.  The `LSL:fullinfo` handler runs
    // on the **data port** (TCP server), not the service port (UDP time sync).
    let addr = {
        let v4 = info.v4address();
        let v4port = info.v4data_port();
        if !v4.is_empty() && v4port > 0 {
            format!("{v4}:{v4port}")
        } else {
            let v6 = info.v6address();
            let v6port = info.v6data_port();
            if !v6.is_empty() && v6port > 0 {
                format!("[{v6}]:{v6port}")
            } else {
                return None;
            }
        }
    };

    let stream = match TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()),
        Duration::from_secs(3),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[lsl] fullinfo TCP connect to {addr} failed: {e}");
            return None;
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
    let mut writer = stream.try_clone().unwrap_or_else(|_| {
        // Should not happen on any supported platform.
        panic!("TcpStream::try_clone failed");
    });

    // Send request.
    if writer.write_all(b"LSL:fullinfo\r\n").is_err() {
        return None;
    }
    let _ = writer.flush();

    // Read response (full-info XML, terminated by `</info>`).
    let reader = std::io::BufReader::new(stream);
    let mut xml = String::with_capacity(4096);
    for line in reader.lines() {
        match line {
            Ok(l) => {
                xml.push_str(&l);
                xml.push('\n');
                if l.contains("</info>") {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    (!xml.is_empty()).then_some(xml)
}

/// Extract channel labels from a full-info XML string by simple tag parsing.
fn parse_labels_from_xml(xml: &str, channel_count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(channel_count);
    // Find all <label>...</label> pairs inside the <channels> section.
    let channels_start = xml.find("<channels>");
    let channels_end = xml.find("</channels>");
    if let (Some(start), Some(end)) = (channels_start, channels_end) {
        let section = &xml[start..end];
        let mut pos = 0;
        while let Some(open) = section[pos..].find("<label>") {
            let label_start = pos + open + 7; // length of "<label>"
            if let Some(close) = section[label_start..].find("</label>") {
                let label = &section[label_start..label_start + close];
                names.push(if label.is_empty() {
                    format!("Ch{}", names.len() + 1)
                } else {
                    label.to_string()
                });
                pos = label_start + close + 8; // length of "</label>"
            } else {
                break;
            }
        }
    }
    names
}

#[async_trait]
impl DeviceAdapter for LslAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    async fn next_event(&mut self) -> Option<DeviceEvent> {
        self.rx.recv().await
    }
    async fn disconnect(&mut self) {}
    fn eeg_scale_uv(&self) -> Option<f64> {
        self.eeg_scale_uv
    }
}
