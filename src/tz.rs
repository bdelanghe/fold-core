//! Time zones. With the `tzdb` feature (the default, and the native library) the zone rules are chrono-tz's. Without it (the
//! WebAssembly module) they are the host's: the module imports `fold_core.tz_offset(zone_ptr, zone_len, epoch_ms) -> seconds`,
//! which js/fold-core.ts answers from `Intl`, the same rules fold.ts reads, so the module carries no zone data (most of its size).
//! `i32::MIN` from the host is a zone it does not know.

use chrono::{Duration, NaiveDateTime};

#[derive(Clone)]
pub enum Zone {
    Utc,
    #[cfg(feature = "tzdb")]
    Db(chrono_tz::Tz),
    #[cfg(not(feature = "tzdb"))]
    Host(String),
}

#[cfg(all(not(feature = "tzdb"), not(target_arch = "wasm32")))]
compile_error!("without the tzdb feature the zone rules come from a WebAssembly host: build for wasm32, or enable tzdb");

#[cfg(all(not(feature = "tzdb"), target_arch = "wasm32"))]
mod host {
    #[link(wasm_import_module = "fold_core")]
    extern "C" {
        fn tz_offset(zone: *const u8, len: usize, epoch_ms: f64) -> i32;
    }

    pub fn offset(zone: &str, epoch_ms: f64) -> Option<i64> {
        // SAFETY: the host reads `len` bytes at `zone`, which live for the call.
        let s = unsafe { tz_offset(zone.as_ptr(), zone.len(), epoch_ms) };
        (s != i32::MIN).then_some(s as i64)
    }
}

impl Zone {
    /// An IANA zone by name, if this build knows it.
    pub fn named(name: &str) -> Option<Zone> {
        #[cfg(feature = "tzdb")]
        return name.parse().ok().map(Zone::Db);
        #[cfg(not(feature = "tzdb"))]
        return host::offset(name, 0.0).map(|_| Zone::Host(name.to_string()));
    }

    /// Seconds east of UTC at a UTC time.
    pub fn offset(&self, utc: &NaiveDateTime) -> i64 {
        match self {
            Zone::Utc => 0,
            #[cfg(feature = "tzdb")]
            Zone::Db(tz) => {
                use chrono::{Offset, TimeZone};
                tz.offset_from_utc_datetime(utc).fix().local_minus_utc() as i64
            }
            #[cfg(not(feature = "tzdb"))]
            Zone::Host(name) => host::offset(name, utc.and_utc().timestamp_millis() as f64).unwrap_or(0),
        }
    }

    /// The wall time here at a UTC time.
    pub fn local(&self, utc: &NaiveDateTime) -> NaiveDateTime {
        *utc + Duration::seconds(self.offset(utc))
    }

    /// The UTC time of a wall time here, as JS reads one: in an overlap the earlier, in a gap the offset from before it (so the
    /// time moves forward by the gap).
    pub fn to_utc(&self, local: &NaiveDateTime) -> NaiveDateTime {
        let day = Duration::days(1);
        let (before, after) = (self.offset(&(*local - day)), self.offset(&(*local + day)));
        let fits = |o: i64| {
            let u = *local - Duration::seconds(o);
            (self.offset(&u) == o).then_some(u)
        };
        [fits(before), fits(after)].into_iter().flatten().min().unwrap_or(*local - Duration::seconds(before))
    }
}
