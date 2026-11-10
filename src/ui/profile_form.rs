//! The patient profile form, shown in a dialog for new patients and edits.

use gpui_kit::component::form::{field, v_form};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, Subscription, Window};

use crate::dates;
use crate::i18n::t;
use crate::model::{Patient, PatientErrors, PatientInput, PatientProfile};
use crate::ui::widgets;

pub enum ProfileFormEvent {
    /// The form was submitted and is valid.
    Submit(PatientProfile),
}

pub struct ProfileForm {
    name: Entity<InputState>,
    head_family_name: Entity<InputState>,
    birth_date: Entity<InputState>,
    address: Entity<TextareaState>,
    errors: PatientErrors,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProfileFormEvent> for ProfileForm {}

impl ProfileForm {
    /// A form for a new patient, or for editing `patient`.
    pub fn new(patient: Option<&Patient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let value =
            |get: fn(&Patient) -> &String| patient.map(|p| get(p).clone()).unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(value(|p| &p.name)));
        let head_family_name =
            cx.new(|cx| InputState::new(window, cx).default_value(value(|p| &p.head_family_name)));
        let birth_date = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("YYYY-MM-DD")
                .default_value(value(|p| &p.birth_date))
        });
        let address = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .default_value(value(|p| &p.address))
        });

        // Enter in a single-line field saves, as in a native dialog.
        let submit_on_enter = |this: &mut Self,
                               _: &Entity<InputState>,
                               event: &InputEvent,
                               window: &mut Window,
                               cx: &mut Context<Self>| {
            if let InputEvent::PressEnter { .. } = event {
                this.submit(window, cx);
            }
        };
        let subscriptions = vec![
            cx.subscribe_in(&name, window, submit_on_enter),
            cx.subscribe_in(&head_family_name, window, submit_on_enter),
            cx.subscribe_in(&birth_date, window, submit_on_enter),
        ];

        Self {
            name,
            head_family_name,
            birth_date,
            address,
            errors: PatientErrors::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Validates the form, and emits [`ProfileFormEvent::Submit`] when valid.
    pub fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let input = PatientInput {
            name: self.name.read(cx).value().to_string(),
            head_family_name: self.head_family_name.read(cx).value().to_string(),
            birth_date: self.birth_date.read(cx).value().to_string(),
            address: self.address.read(cx).value().to_string(),
        };
        match input.validate(dates::today()) {
            Ok(profile) => {
                self.errors = PatientErrors::default();
                cx.emit(ProfileFormEvent::Submit(profile));
            }
            Err(errors) => self.errors = errors,
        }
        cx.notify();
    }
}

impl Render for ProfileForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let with_error = |input: AnyElement, error: Option<&String>, cx: &Context<Self>| {
            v_flex()
                .w_full()
                .child(input)
                .children(widgets::field_error(error, cx))
        };
        v_form()
            .child(
                field()
                    .label(t("Nama", "Name"))
                    .required(true)
                    .child(with_error(
                        Input::new(&self.name).into_any_element(),
                        self.errors.name.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Nama KK", "Head of Family"))
                    .child(with_error(
                        Input::new(&self.head_family_name).into_any_element(),
                        self.errors.head_family_name.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Tanggal Lahir", "Date of Birth"))
                    .required(true)
                    .description(t(
                        "YYYY-MM-DD, atau DD-MM-YYYY",
                        "YYYY-MM-DD, or DD-MM-YYYY",
                    ))
                    .child(with_error(
                        Input::new(&self.birth_date).into_any_element(),
                        self.errors.birth_date.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Alamat", "Address"))
                    .required(true)
                    .child(with_error(
                        Textarea::new(&self.address).into_any_element(),
                        self.errors.address.as_ref(),
                        cx,
                    )),
            )
    }
}
