//! Shared presentation and editors for typed frontmatter properties.
//!
//! The configured-defaults dialog, the per-note properties dialog, and the Bases grid all label
//! the same [`PropertyType`] values and edit dates the same way, so the translation and the date
//! picker live here rather than in each surface.

use std::{cell::RefCell, rc::Rc};

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

/// A calendar (and optional time) picker for a date or date-time value.
///
/// The widget is a flat menu button whose popover holds a calendar, optional time spins, and
/// Clear/Done actions. It is shared by the properties dialog (as an `ActionRow` suffix) and the
/// Bases grid (as an inline cell editor).
#[derive(Clone)]
pub(crate) struct DatePicker {
    button: gtk::MenuButton,
    popover: gtk::Popover,
    calendar: gtk::Calendar,
    hours: gtk::SpinButton,
    minutes: gtk::SpinButton,
    field_type: PropertyType,
    state: Rc<RefCell<Option<String>>>,
    on_changed: ChangeCallback,
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

        let hours = gtk::SpinButton::with_range(0.0, 23.0, 1.0);
        let minutes = gtk::SpinButton::with_range(0.0, 59.0, 1.0);
        hours.set_widget_name(&format!("{name}-hours"));
        minutes.set_widget_name(&format!("{name}-minutes"));
        hours.set_width_chars(2);
        minutes.set_width_chars(2);
        if !date_only {
            let clock = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            clock.set_halign(gtk::Align::Center);
            clock.append(&hours);
            clock.append(&gtk::Label::new(Some(":")));
            clock.append(&minutes);
            content.append(&clock);
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
        apply_picker_state(field_type, &state, &calendar, &hours, &minutes);

        let update: Rc<dyn Fn()> = {
            let calendar = calendar.clone();
            let hours = hours.clone();
            let minutes = minutes.clone();
            let state = Rc::clone(&state);
            let on_changed = Rc::clone(&on_changed);
            Rc::new(move || {
                *state.borrow_mut() = read_picker(field_type, &calendar, &hours, &minutes);
                if let Some(callback) = on_changed.borrow().as_ref() {
                    callback();
                }
            })
        };
        {
            let update = Rc::clone(&update);
            calendar.connect_day_selected(move |_| update());
        }
        if !date_only {
            let update_hours = Rc::clone(&update);
            hours.connect_value_changed(move |_| update_hours());
            let update_minutes = Rc::clone(&update);
            minutes.connect_value_changed(move |_| update_minutes());
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
            hours,
            minutes,
            field_type,
            state,
            on_changed,
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
        *self.state.borrow_mut() = picker_value(self.field_type, value);
        apply_picker_state(
            self.field_type,
            &self.state,
            &self.calendar,
            &self.hours,
            &self.minutes,
        );
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
    hours: &gtk::SpinButton,
    minutes: &gtk::SpinButton,
) {
    let borrow = state.borrow();
    let Some(iso) = borrow.as_deref() else {
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
    hours.set_value(f64::from(local.hour()));
    minutes.set_value(f64::from(local.minute()));
}

/// Reads the picker's calendar and time into an ISO 8601 string.
fn read_picker(
    field_type: PropertyType,
    calendar: &gtk::Calendar,
    hours: &gtk::SpinButton,
    minutes: &gtk::SpinButton,
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
    // Build the selected wall clock in the local zone so the stored offset matches that date.
    let local = glib::DateTime::new(
        &glib::TimeZone::local(),
        selected.year(),
        selected.month(),
        selected.day_of_month(),
        hours.value_as_int(),
        minutes.value_as_int(),
        0.0,
    )
    .ok()?;
    let offset =
        time::UtcOffset::from_whole_seconds(i32::try_from(local.utc_offset().as_seconds()).ok()?)
            .ok()?;
    let time = time::Time::from_hms(
        u8::try_from(local.hour()).ok()?,
        u8::try_from(local.minute()).ok()?,
        0,
    )
    .ok()?;
    time::PrimitiveDateTime::new(date, time)
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
