use std::fmt;
use time::macros::format_description;
use tracing_core::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{
        format::{self, FormatEvent, FormatFields},
        time::FormatTime,
        time::UtcTime,
        FmtContext, FormattedFields,
    },
    registry::LookupSpan,
};

/// Mirrors the OpenSips log format with small tweaks for Rust
///
/// MON DD HH:MM:SS [PID] LEVEL:TARGET: <message>
struct OpenSipsFormat {
    timer: UtcTime<&'static [time::format_description::FormatItem<'static>]>,
}

impl OpenSipsFormat {
    fn new() -> Self {
        let timer = UtcTime::new(format_description!(
            "[month repr:short] [day] [hour repr:24]:[minute]:[second]"
        ));

        Self { timer }
    }
}

// Copied and modified from
// https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/trait.FormatEvent.html#examples
impl<S, N> FormatEvent<S, N> for OpenSipsFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        self.timer.format_time(&mut writer)?;

        // Since OpenSips modules are forked, we can't get the PID
        // once and cache it.
        let pid = std::process::id();
        let metadata = event.metadata();
        let level = metadata.level();
        let target = metadata.target();

        write!(writer, " [{pid}] {level}:{target}: ")?;

        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                write!(writer, "{}", span.name())?;

                let ext = span.extensions();
                let fields = &ext
                    .get::<FormattedFields<N>>()
                    .expect("will never be `None`");

                if !fields.is_empty() {
                    write!(writer, "{{{}}}", fields)?;
                }
                write!(writer, ": ")?;
            }
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

pub fn install() {
    tracing_subscriber::fmt()
        .event_format(OpenSipsFormat::new())
        .with_writer(std::io::stderr)
        .init()
}
