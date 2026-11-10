//! Domain types and input validation.

use chrono::NaiveDate;

use crate::dates;
use crate::i18n::t;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub head_family_name: String,
    /// `YYYY-MM-DD`
    pub birth_date: String,
    pub address: String,
    /// `YYYY-MM-DD`, the most recent visit (or the registration date).
    pub last_visit: String,
}

/// How a visit's diagnosis text is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosisFormat {
    /// Written with the rich text editor of the original app.
    Html,
    /// Written with this app.
    Markdown,
}

impl DiagnosisFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosisFormat::Html => "html",
            DiagnosisFormat::Markdown => "markdown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "html" => Some(DiagnosisFormat::Html),
            "markdown" => Some(DiagnosisFormat::Markdown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub id: String,
    pub patient_id: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub diagnosis: String,
    pub format: DiagnosisFormat,
}

/// A visit in the list of all visits, without its full text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisitSummary {
    pub id: String,
    pub patient_id: String,
    pub patient_name: String,
    pub head_family_name: String,
    /// `YYYY-MM-DD`
    pub date: String,
    /// The first lines of the diagnosis, as plain text.
    pub summary: String,
}

/// How the list of patients is sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatientOrder {
    /// Most recently visited first.
    #[default]
    RecentVisit,
    /// Alphabetically by name.
    Name,
}

/// A page of results and the cursor of the next page, if there is one.
#[derive(Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<Cursor>,
}

/// Keyset pagination cursor: results continue strictly after
/// (`date`, `id`) in descending order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub date: String,
    pub id: String,
}

/// Raw values of the patient profile form.
#[derive(Clone, Debug, Default)]
pub struct PatientInput {
    pub name: String,
    pub head_family_name: String,
    pub birth_date: String,
    pub address: String,
}

/// A validated patient profile, ready to be stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatientProfile {
    pub name: String,
    pub head_family_name: String,
    pub birth_date: NaiveDate,
    pub address: String,
}

/// Per-field validation messages of the profile form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PatientErrors {
    pub name: Option<String>,
    pub head_family_name: Option<String>,
    pub birth_date: Option<String>,
    pub address: Option<String>,
}

impl PatientErrors {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.head_family_name.is_none()
            && self.birth_date.is_none()
            && self.address.is_none()
    }
}

fn check_length(value: &str, max: usize) -> Option<String> {
    (value.chars().count() > max).then(|| {
        crate::i18n::pick(
            format!("Maksimal {max} karakter"),
            format!("Maximum at {max} characters"),
        )
    })
}

impl PatientInput {
    /// Validates the form with the same rules as the original app.
    pub fn validate(&self, today: NaiveDate) -> Result<PatientProfile, PatientErrors> {
        let name = self.name.trim();
        let head_family_name = self.head_family_name.trim();
        let address = self.address.trim();

        let mut errors = PatientErrors {
            name: if name.is_empty() {
                Some(t("Wajib diisi", "Required").into())
            } else {
                check_length(name, 255)
            },
            head_family_name: check_length(head_family_name, 255),
            birth_date: None,
            address: if address.is_empty() {
                Some(t("Wajib diisi", "Required").into())
            } else {
                check_length(address, 4096)
            },
        };

        let birth_date = match dates::parse_user_date(&self.birth_date) {
            Ok(date) if date > today => {
                errors.birth_date = Some(
                    t(
                        "Tanggal lahir ada di masa depan",
                        "Birthdate is in the future",
                    )
                    .into(),
                );
                None
            }
            Ok(date) => Some(date),
            Err(message) => {
                errors.birth_date = Some(message);
                None
            }
        };

        match birth_date {
            Some(birth_date) if errors.is_empty() => Ok(PatientProfile {
                name: name.to_string(),
                head_family_name: head_family_name.to_string(),
                birth_date,
                address: address.to_string(),
            }),
            _ => Err(errors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    #[test]
    fn valid_input_is_trimmed() {
        let input = PatientInput {
            name: "  Siti  Aminah ".into(),
            head_family_name: " Joko ".into(),
            birth_date: "1990-1-5".into(),
            address: " Jl. Mawar 1 ".into(),
        };
        let profile = input.validate(today()).unwrap();
        assert_eq!(profile.name, "Siti  Aminah");
        assert_eq!(profile.head_family_name, "Joko");
        assert_eq!(
            profile.birth_date,
            NaiveDate::from_ymd_opt(1990, 1, 5).unwrap()
        );
        assert_eq!(profile.address, "Jl. Mawar 1");
    }

    #[test]
    fn reports_every_invalid_field() {
        let input = PatientInput {
            name: " ".into(),
            head_family_name: "x".repeat(256),
            birth_date: "2030-01-01".into(),
            address: "".into(),
        };
        let errors = input.validate(today()).unwrap_err();
        assert_eq!(errors.name.as_deref(), Some("Wajib diisi"));
        assert!(errors.head_family_name.is_some());
        assert_eq!(
            errors.birth_date.as_deref(),
            Some("Tanggal lahir ada di masa depan")
        );
        assert_eq!(errors.address.as_deref(), Some("Wajib diisi"));
    }

    #[test]
    fn head_family_name_is_optional() {
        let input = PatientInput {
            name: "Budi".into(),
            head_family_name: "".into(),
            birth_date: "2000-02-29".into(),
            address: "Desa".into(),
        };
        assert!(input.validate(today()).is_ok());
    }
}
