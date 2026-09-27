//! Shared presentation and editors for typed frontmatter properties.
//!
//! The configured-defaults dialog, the per-note properties dialog, and the Bases grid all label
//! the same [`PropertyType`] values and edit dates the same way, so the translation and the date
//! picker live here rather than in each surface.

use std::{cell::Cell, cell::RefCell, rc::Rc};

use carver_domain::{FrontmatterValue, PropertyType, parse_iso_date, parse_iso_date_time};
use carver_sdk::PropertyPath;
use gettextrs::gettext;
use gtk::prelude::*;

/// Returns the user-visible label for a property type.
#[must_use]
pub(crate) fn property_type_label(field_type: PropertyType) -> String {
    match field_type {
        PropertyType::Text => gettext("Text"),
        PropertyType::LongText => gettext("Long text"),
        PropertyType::Number => gettext("Number"),
        PropertyType::Boolean => gettext("Boolean"),
        PropertyType::List => gettext("List"),
        PropertyType::Date => gettext("Date"),
        PropertyType::DateTime => gettext("Date & time"),
    }
}

/// Builds a JSON Pointer path for a top-level configured property key.
#[must_use]
pub(crate) fn property_path(key: &str) -> PropertyPath {
    PropertyPath(format!(
        "/{}",
        key.trim().replace('~', "~0").replace('/', "~1")
    ))
}

/// Renders a frontmatter value as plain text for an entry editor or a preview.
#[must_use]
pub(crate) fn frontmatter_text(value: &FrontmatterValue) -> String {
    match value {
        FrontmatterValue::Text(text) => text.clone(),
        FrontmatterValue::Number(number) => number.to_string(),
        FrontmatterValue::Boolean(flag) => flag.to_string(),
        FrontmatterValue::Null => String::new(),
        FrontmatterValue::Object(_) | FrontmatterValue::List(_) => value.to_json().to_string(),
    }
}

/// Parses an integer or float entry into a JSON number.
#[must_use]
pub(crate) fn parse_number(text: &str) -> Option<serde_json::Number> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return Some(serde_json::Number::from(value));
    }
    if let Ok(value) = trimmed.parse::<u64>() {
        return Some(serde_json::Number::from(value));
    }
    trimmed
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
}

/// The callback registered to observe picker changes.
type ChangeCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// A GNOME-style hour/minute spinner with chevron controls.
///
/// The displayed shape follows the system clock format: 24-hour (`17:24`) or 12-hour with an
/// AM/PM toggle (`5:24 PM`). The value is always kept as a 24-hour hour and minute.
#[derive(Clone)]
pub(crate) struct TimeSpinner {
    container: gtk::Box,
    hour: Rc<Cell<u8>>,
    minute: Rc<Cell<u8>>,
    refresh: Rc<dyn Fn()>,
    on_changed: ChangeCallback,
}

impl TimeSpinner {
    /// Builds a spinner seeded at midnight, with controls named from `name`.
    // CONTEXT: The digit columns, chevron controls, AM/PM toggle, and their state live together.
    #[expect(
        clippy::too_many_lines,
        reason = "the spinner builds its columns and wiring in one place"
    )]
    #[must_use]
    pub(crate) fn new(name: &str) -> Self {
        let twelve_hour = uses_twelve_hour_clock();
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        container.set_widget_name(&format!("{name}-time"));
        container.set_halign(gtk::Align::Center);
        container.add_css_class("time-spinner");

        let hour = Rc::new(Cell::new(0u8));
        let minute = Rc::new(Cell::new(0u8));
        let on_changed: ChangeCallback = Rc::new(RefCell::new(None));

        let hour_label = time_value_label(&format!("{name}-hour-label"));
        let minute_label = time_value_label(&format!("{name}-minute-label"));

        // The AM/PM text comes from the locale, so it matches how the system shows times.
        let (am_label, pm_label) = if twelve_hour {
            meridiem_labels()
        } else {
            (String::new(), String::new())
        };
        let meridiem = twelve_hour.then(|| {
            let button = gtk::Button::with_label(&am_label);
            button.set_widget_name(&format!("{name}-meridiem"));
            button.add_css_class("flat");
            button.add_css_class("time-spinner-meridiem");
            button.update_property(&[gtk::accessible::Property::Label(&gettext("AM or PM"))]);
            button
        });

        let refresh: Rc<dyn Fn()> = {
            let hour = Rc::clone(&hour);
            let minute = Rc::clone(&minute);
            let hour_label = hour_label.clone();
            let minute_label = minute_label.clone();
            let meridiem = meridiem.clone();
            let am_label = am_label.clone();
            let pm_label = pm_label.clone();
            Rc::new(move || {
                let value = hour.get();
                let display_hour = if twelve_hour {
                    match value % 12 {
                        0 => 12,
                        other => other,
                    }
                } else {
                    value
                };
                hour_label.set_text(&format!("{display_hour:02}"));
                minute_label.set_text(&format!("{:02}", minute.get()));
                if let Some(meridiem) = &meridiem {
                    meridiem.set_label(if value < 12 { &am_label } else { &pm_label });
                }
            })
        };

        container.append(&time_column(
            &hour_label,
            &step_button(
                &format!("{name}-hour-up"),
                "pan-up-symbolic",
                &gettext("Increase hours"),
                {
                    let hour = Rc::clone(&hour);
                    let refresh = Rc::clone(&refresh);
                    let on_changed = Rc::clone(&on_changed);
                    move || {
                        hour.set(hour.get().wrapping_add(1) % 24);
                        refresh();
                        notify_changed(&on_changed);
                    }
                },
            ),
            &step_button(
                &format!("{name}-hour-down"),
                "pan-down-symbolic",
                &gettext("Decrease hours"),
                {
                    let hour = Rc::clone(&hour);
                    let refresh = Rc::clone(&refresh);
                    let on_changed = Rc::clone(&on_changed);
                    move || {
                        hour.set(hour.get().wrapping_add(23) % 24);
                        refresh();
                        notify_changed(&on_changed);
                    }
                },
            ),
        ));
        let colon = gtk::Label::new(Some(":"));
        colon.add_css_class("time-spinner-value");
        container.append(&colon);
        container.append(&time_column(
            &minute_label,
            &step_button(
                &format!("{name}-minute-up"),
                "pan-up-symbolic",
                &gettext("Increase minutes"),
                {
                    let minute = Rc::clone(&minute);
                    let refresh = Rc::clone(&refresh);
                    let on_changed = Rc::clone(&on_changed);
                    move || {
                        minute.set(minute.get().wrapping_add(1) % 60);
                        refresh();
                        notify_changed(&on_changed);
                    }
                },
            ),
            &step_button(
                &format!("{name}-minute-down"),
                "pan-down-symbolic",
                &gettext("Decrease minutes"),
                {
                    let minute = Rc::clone(&minute);
                    let refresh = Rc::clone(&refresh);
                    let on_changed = Rc::clone(&on_changed);
                    move || {
                        minute.set(minute.get().wrapping_add(59) % 60);
                        refresh();
                        notify_changed(&on_changed);
                    }
                },
            ),
        ));
        if let Some(meridiem) = &meridiem {
            let hour = Rc::clone(&hour);
            let refresh = Rc::clone(&refresh);
            let on_changed = Rc::clone(&on_changed);
            meridiem.connect_clicked(move |_| {
                let value = hour.get();
                hour.set(if value < 12 { value + 12 } else { value - 12 });
                refresh();
                notify_changed(&on_changed);
            });
            container.append(meridiem);
        }
        refresh();

        Self {
            container,
            hour,
            minute,
            refresh,
            on_changed,
        }
    }

    /// Returns the spinner row for the picker popover.
    pub(crate) fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Registers a callback invoked after any time change.
    pub(crate) fn connect_changed(&self, callback: impl Fn() + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// Sets the 24-hour time without notifying, used while reseeding.
    pub(crate) fn set_time(&self, hour: u8, minute: u8) {
        self.hour.set(hour % 24);
        self.minute.set(minute % 60);
        (self.refresh)();
    }

    /// Returns the current 24-hour time.
    pub(crate) fn time(&self) -> (u8, u8) {
        (self.hour.get(), self.minute.get())
    }
}

fn time_value_label(name: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_widget_name(name);
    label.add_css_class("time-spinner-value");
    label.set_halign(gtk::Align::Center);
    label
}

fn time_column(value: &gtk::Label, up: &gtk::Button, down: &gtk::Button) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_halign(gtk::Align::Center);
    column.append(up);
    column.append(value);
    column.append(down);
    column
}

fn step_button(name: &str, icon: &str, label: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.add_css_class("time-spinner-step");
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button.connect_clicked(move |_| on_click());
    button
}

fn notify_changed(on_changed: &ChangeCallback) {
    if let Some(callback) = on_changed.borrow().as_ref() {
        callback();
    }
}

/// Returns whether the system prefers a 12-hour clock.
///
/// Under GNOME the desktop's explicit clock format wins, matching what the user sees elsewhere.
/// Everywhere else (KDE, sway, a bare session, GNOME settings not installed) the locale's time
/// format decides, so Carver never applies a GNOME default that does not reflect the system.
fn uses_twelve_hour_clock() -> bool {
    if is_gnome_session()
        && let Some(format) = gnome_clock_format()
    {
        match format.as_str() {
            "12h" => return true,
            "24h" => return false,
            _ => {}
        }
    }
    locale_uses_twelve_hour_clock()
}

/// Returns whether the session is GNOME, from the freedesktop session environment.
fn is_gnome_session() -> bool {
    ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "GDMSESSION"]
        .iter()
        .any(|key| {
            std::env::var(key).is_ok_and(|value| {
                value
                    .split(':')
                    .any(|token| token.eq_ignore_ascii_case("GNOME"))
            })
        })
}

/// Reads the GNOME clock format when the settings schema is installed.
///
/// `gio::Settings::new` panics for a missing schema, so the schema is looked up first; a system
/// without `gsettings-desktop-schemas` returns `None` and the locale fallback is used.
fn gnome_clock_format() -> Option<String> {
    gtk::gio::SettingsSchemaSource::default()?.lookup("org.gnome.desktop.interface", true)?;
    Some(
        gtk::gio::Settings::new("org.gnome.desktop.interface")
            .string("clock-format")
            .to_string(),
    )
}

/// Infers 12-hour vs 24-hour from the locale's own time format.
fn locale_uses_twelve_hour_clock() -> bool {
    glib::DateTime::from_local(2026, 1, 1, 13, 0, 0.0)
        .and_then(|when| when.format("%X"))
        .is_ok_and(|formatted| !formatted.contains("13"))
}

/// Returns the locale's own AM and PM labels, falling back to the English tokens.
fn meridiem_labels() -> (String, String) {
    let am = glib::DateTime::from_local(2026, 1, 1, 9, 0, 0.0)
        .and_then(|when| when.format("%p"))
        .map_or_else(|_| "AM".to_owned(), |label| label.to_string());
    let pm = glib::DateTime::from_local(2026, 1, 1, 21, 0, 0.0)
        .and_then(|when| when.format("%p"))
        .map_or_else(|_| "PM".to_owned(), |label| label.to_string());
    (am, pm)
}

/// A calendar (and optional time) picker for a date or date-time value.
///
/// The widget is a flat menu button whose popover holds a calendar, an optional time spinner, and
/// Clear/Done actions. It is shared by the properties dialog (as an `ActionRow` suffix) and the
/// Bases grid (as an inline cell editor).
#[derive(Clone)]
pub(crate) struct DatePicker {
    button: gtk::MenuButton,
    popover: gtk::Popover,
    calendar: gtk::Calendar,
    time: TimeSpinner,
    field_type: PropertyType,
    state: Rc<RefCell<Option<String>>>,
    on_changed: ChangeCallback,
    /// Set while programmatically reseeding, so widget signals do not write back mid-apply.
    applying: Rc<Cell<bool>>,
    date_only: bool,
}

impl DatePicker {
    /// Builds a picker for `field_type`, seeded from an existing frontmatter value.
    pub(crate) fn new(field_type: PropertyType, value: &FrontmatterValue, name: &str) -> Self {
        let date_only = field_type == PropertyType::Date;
        let button = gtk::MenuButton::new();
        button.set_icon_name("x-office-calendar-symbolic");
        button.add_css_class("flat");
        button.set_valign(gtk::Align::Center);
        button.set_widget_name(&format!("{name}-picker"));
        let popover = gtk::Popover::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_start(6);
        content.set_margin_end(6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        let calendar = gtk::Calendar::new();
        calendar.set_widget_name(&format!("{name}-calendar"));
        content.append(&calendar);

        let time = TimeSpinner::new(name);
        if !date_only {
            content.append(time.widget());
        }

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let clear = gtk::Button::with_label(&gettext("Clear"));
        clear.add_css_class("flat");
        clear.set_halign(gtk::Align::Start);
        clear.set_hexpand(true);
        clear.set_widget_name(&format!("{name}-clear"));
        let done = gtk::Button::with_label(&gettext("Done"));
        done.add_css_class("suggested-action");
        done.set_halign(gtk::Align::End);
        done.set_widget_name(&format!("{name}-done"));
        actions.append(&clear);
        actions.append(&done);
        content.append(&actions);

        popover.set_child(Some(&content));
        popover.set_widget_name(&format!("{name}-popover"));
        button.set_popover(Some(&popover));

        let state = Rc::new(RefCell::new(picker_value(field_type, value)));
        let on_changed: ChangeCallback = Rc::new(RefCell::new(None));
        let applying = Rc::new(Cell::new(false));
        apply_picker_state(field_type, &state, &calendar, &time);

        let update: Rc<dyn Fn()> = {
            let calendar = calendar.clone();
            let time = time.clone();
            let state = Rc::clone(&state);
            let on_changed = Rc::clone(&on_changed);
            let applying = Rc::clone(&applying);
            Rc::new(move || {
                if applying.get() {
                    return;
                }
                *state.borrow_mut() = read_picker(field_type, &calendar, &time);
                if let Some(callback) = on_changed.borrow().as_ref() {
                    callback();
                }
            })
        };
        {
            let update = Rc::clone(&update);
            calendar.connect_day_selected(move |_| update());
        }
        {
            let update = Rc::clone(&update);
            time.connect_changed(move || update());
        }
        {
            let state = Rc::clone(&state);
            let on_changed = Rc::clone(&on_changed);
            clear.connect_clicked(move |_| {
                *state.borrow_mut() = None;
                if let Some(callback) = on_changed.borrow().as_ref() {
                    callback();
                }
            });
        }
        {
            let popover = popover.clone();
            done.connect_clicked(move |_| popover.popdown());
        }

        Self {
            button,
            popover,
            calendar,
            time,
            field_type,
            state,
            on_changed,
            applying,
            date_only,
        }
    }

    /// Returns the picker's button so a preferred row can place it as a suffix.
    pub(crate) fn button(&self) -> &gtk::MenuButton {
        &self.button
    }

    /// Returns the picker's popover so a caller can observe opening and closing.
    pub(crate) fn popover(&self) -> &gtk::Popover {
        &self.popover
    }

    /// Registers a callback invoked after any picker change, including Clear.
    pub(crate) fn connect_changed(&self, callback: impl Fn() + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    /// Reseeds the picker from a frontmatter value and repaints its calendar and time controls.
    ///
    /// Used by a grid cell that opens the picker for a different row each time.
    pub(crate) fn set_frontmatter(&self, value: &FrontmatterValue) {
        self.applying.set(true);
        *self.state.borrow_mut() = picker_value(self.field_type, value);
        apply_picker_state(self.field_type, &self.state, &self.calendar, &self.time);
        self.applying.set(false);
    }

    /// Returns the current ISO 8601 value, or `None` when cleared.
    pub(crate) fn value(&self) -> Option<String> {
        self.state.borrow().clone()
    }

    /// Sets the current value and notifies, when a callback is registered.
    pub(crate) fn set_value(&self, value: Option<&str>) {
        *self.state.borrow_mut() = value.map(ToOwned::to_owned);
        if let Some(callback) = self.on_changed.borrow().as_ref() {
            callback();
        }
    }

    /// Returns the frontmatter value for the current picker state.
    pub(crate) fn frontmatter(&self) -> FrontmatterValue {
        self.value()
            .map_or(FrontmatterValue::Null, FrontmatterValue::Text)
    }

    /// Returns whether the picker edits a date without a time.
    pub(crate) fn is_date_only(&self) -> bool {
        self.date_only
    }
}

/// Returns the value text for a picker, or `None` when it is absent or unparseable.
pub(crate) fn picker_value(field_type: PropertyType, value: &FrontmatterValue) -> Option<String> {
    let FrontmatterValue::Text(text) = value else {
        return None;
    };
    let valid = if field_type == PropertyType::Date {
        parse_iso_date(text).is_some()
    } else {
        parse_iso_date_time(text).is_some()
    };
    valid.then(|| text.clone())
}

/// Applies a picker state to its calendar and time spins.
fn apply_picker_state(
    field_type: PropertyType,
    state: &Rc<RefCell<Option<String>>>,
    calendar: &gtk::Calendar,
    spinner: &TimeSpinner,
) {
    // Clone and drop the borrow before mutating widgets: setting the calendar emits
    // `day-selected`, whose handler writes back to the same state.
    let iso = state.borrow().clone();
    let Some(iso) = iso.as_deref() else {
        return;
    };
    if field_type == PropertyType::Date {
        if let Some(date) = parse_iso_date(iso) {
            set_calendar(calendar, date);
        }
        return;
    }
    let Some(instant) = parse_iso_date_time(iso) else {
        return;
    };
    // Convert the stored instant to the local wall clock, which resolves the offset for that
    // instant (honouring daylight-saving shifts) rather than assuming today's offset.
    let Ok(local) = glib::DateTime::from_unix_local(instant.unix_timestamp()) else {
        return;
    };
    calendar.set_year(local.year());
    calendar.set_month(local.month() - 1);
    calendar.set_day(local.day_of_month());
    spinner.set_time(
        u8::try_from(local.hour()).unwrap_or(0),
        u8::try_from(local.minute()).unwrap_or(0),
    );
}

/// Reads the picker's calendar and time into an ISO 8601 string.
fn read_picker(
    field_type: PropertyType,
    calendar: &gtk::Calendar,
    spinner: &TimeSpinner,
) -> Option<String> {
    let selected = calendar.date();
    let date = time::Date::from_calendar_date(
        selected.year(),
        time::Month::try_from(u8::try_from(selected.month()).ok()?).ok()?,
        u8::try_from(selected.day_of_month()).ok()?,
    )
    .ok()?;
    if field_type == PropertyType::Date {
        return Some(date.to_string());
    }
    let (hour, minute) = spinner.time();
    // Build the selected wall clock in the local zone so the stored offset matches that date.
    let local = glib::DateTime::new(
        &glib::TimeZone::local(),
        selected.year(),
        selected.month(),
        selected.day_of_month(),
        i32::from(hour),
        i32::from(minute),
        0.0,
    )
    .ok()?;
    let offset =
        time::UtcOffset::from_whole_seconds(i32::try_from(local.utc_offset().as_seconds()).ok()?)
            .ok()?;
    let clock = time::Time::from_hms(hour, minute, 0).ok()?;
    time::PrimitiveDateTime::new(date, clock)
        .assume_offset(offset)
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

fn set_calendar(calendar: &gtk::Calendar, date: time::Date) {
    calendar.set_year(date.year());
    calendar.set_month(i32::from(u8::from(date.month())) - 1);
    calendar.set_day(i32::from(date.day()));
}

/// Formats an ISO 8601 value with the user's locale, falling back to the raw text.
pub(crate) fn display_date(iso: &str, date_only: bool) -> String {
    glib::DateTime::from_iso8601(iso, None)
        .ok()
        .and_then(|date_time| date_time.to_local().ok())
        .and_then(|date_time| {
            date_time
                .format(if date_only { "%x" } else { "%x %X" })
                .ok()
        })
        .map_or_else(|| iso.to_owned(), |formatted| formatted.to_string())
}
