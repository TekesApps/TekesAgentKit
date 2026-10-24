//! Demo host calendar/locale adapter. Agent components own visibility and timing
//! policy; the host supplies system calendar formatting and a display clock.
use tekes_agent_components_ui::{TranscriptContentLabels, TurnFootnoteLabels};

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as f64
        * 1000.
}
pub fn transcript_labels() -> TranscriptContentLabels {
    TranscriptContentLabels {
        input_information: |delivery| match delivery {
            tekes_agent_presentation::TranscriptDelivery::Pending => "未读".into(),
            tekes_agent_presentation::TranscriptDelivery::Recorded { time } => {
                time.and_then(short_clock).unwrap_or_default()
            }
        },
        footnote: TurnFootnoteLabels {
            finished_timestamp: day_clock,
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn short_clock(timestamp: f64) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        native::format(timestamp, None, false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = timestamp;
        None
    }
}
pub fn day_clock(timestamp: f64, now: Option<f64>) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        native::format(timestamp, now, true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (timestamp, now);
        None
    }
}
#[cfg(target_os = "macos")]
mod native {
    use objc2_foundation::{
        NSCalendar, NSCalendarOptions, NSCalendarUnit, NSDate, NSDateFormatter, NSLocale, NSString,
    };
    fn hour_locale(identifier: &str) -> String {
        let (base, keywords) = identifier.split_once('@').unwrap_or((identifier, ""));
        let mut keywords: Vec<_> = keywords
            .split(';')
            .filter(|s| !s.is_empty() && !s.starts_with("hours=") && !s.starts_with("hc="))
            .collect();
        keywords.push("hours=h12");
        format!("{base}@{}", keywords.join(";"))
    }
    pub(super) fn format(timestamp: f64, now: Option<f64>, include_day: bool) -> Option<String> {
        if !timestamp.is_finite()
            || timestamp <= 0.
            || timestamp > NSDate::distantFuture().timeIntervalSince1970() * 1000.
            || now.is_some_and(|n| !n.is_finite() || n <= 0.)
        {
            return None;
        }
        let locale = NSLocale::currentLocale();
        let calendar = NSCalendar::currentCalendar();
        format_in(timestamp, now, include_day, &locale, &calendar)
    }
    fn format_in(
        timestamp: f64,
        now: Option<f64>,
        include_day: bool,
        locale: &NSLocale,
        calendar: &NSCalendar,
    ) -> Option<String> {
        let forced = NSLocale::localeWithLocaleIdentifier(&NSString::from_str(&hour_locale(
            &locale.localeIdentifier().to_string(),
        )));
        let date = NSDate::dateWithTimeIntervalSince1970(timestamp / 1000.);
        let recent = now
            .map(|now| {
                let now = NSDate::dateWithTimeIntervalSince1970(now / 1000.);
                let start = calendar.startOfDayForDate(&date);
                let end = calendar.startOfDayForDate(&now);
                let distance = calendar
                    .components_fromDate_toDate_options(
                        NSCalendarUnit::Day,
                        &start,
                        &end,
                        NSCalendarOptions::empty(),
                    )
                    .day();
                (0..7).contains(&distance)
            })
            .unwrap_or(false);
        let formatter = NSDateFormatter::new();
        formatter.setLocale(Some(&forced));
        formatter.setCalendar(Some(calendar));
        formatter.setTimeZone(Some(&calendar.timeZone()));
        formatter.setLocalizedDateFormatFromTemplate(&NSString::from_str(if !include_day {
            "hmma"
        } else if recent {
            "Ehmma"
        } else {
            "Mdhmma"
        }));
        Some(formatter.stringFromDate(&date).to_string())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn actual_foundation_clock_forces_twelve_hours_and_uses_calendar_day_cutoff() {
            use objc2_foundation::NSTimeZone;
            let calendar =
                NSCalendar::calendarWithIdentifier(&NSString::from_str("gregorian")).unwrap();
            calendar.setTimeZone(&NSTimeZone::timeZoneForSecondsFromGMT(0));
            for id in ["en_US@hours=h23", "en_JP", "zh_CN"] {
                let locale = NSLocale::localeWithLocaleIdentifier(&NSString::from_str(id));
                let clock = format_in(1_700_000_000_000., None, false, &locale, &calendar).unwrap();
                assert!(clock.contains("10:13"), "{id}: {clock}");
                assert!(!clock.contains("22:13"), "{id}: {clock}");
                assert!(
                    clock.contains("PM") || clock.contains("下午"),
                    "{id}: {clock}"
                );
            }
            let locale = NSLocale::localeWithLocaleIdentifier(&NSString::from_str("en_US"));
            calendar.setTimeZone(
                &NSTimeZone::timeZoneWithName(&NSString::from_str("America/Los_Angeles")).unwrap(),
            );
            let ended = 1_699_173_000_000.; // 2023-11-05 01:30 PDT, before DST fallback.
            let recent =
                format_in(ended, Some(1_699_774_200_000.), true, &locale, &calendar).unwrap(); // Nov 11 23:30 PST.
            let old = format_in(ended, Some(1_699_777_800_000.), true, &locale, &calendar).unwrap(); // Nov 12 00:30 PST.
            assert!(recent.contains("Sun"), "{recent}");
            assert!(old.contains("11/5"), "{old}");
        }
        #[test]
        fn invalid_time_stays_unknown_and_hour_keyword_preserves_other_preferences() {
            assert_eq!(format(f64::NAN, Some(1.), true), None);
            assert_eq!(format(0., None, false), None);
            assert_eq!(
                hour_locale("en_JP@calendar=japanese;hours=h23"),
                "en_JP@calendar=japanese;hours=h12"
            );
            assert_eq!(hour_locale("zh_CN"), "zh_CN@hours=h12");
        }
    }
}
