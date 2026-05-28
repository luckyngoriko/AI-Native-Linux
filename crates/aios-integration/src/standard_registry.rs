use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};

use crate::error::IntegrationError;
use crate::evidence::IntegrationEvidenceEmitter;
use crate::ids::StandardSubscriptionId;
use crate::standard::{StandardKind, StandardSubscription};

/// Immutable record of a subscription revision review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardReviewRecord {
    /// The subscription that was reviewed.
    pub subscription_id: StandardSubscriptionId,
    /// When the review was performed.
    pub reviewed_at: DateTime<Utc>,
    /// Canonical identity of the reviewer.
    pub reviewer: String,
    /// Revision before this review.
    pub revision_before: String,
    /// Revision after this review.
    pub revision_after: String,
    /// Free-text note from the reviewer.
    pub note: String,
}

/// Current status of a standard subscription based on `now` vs. review deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionStatus {
    /// Review is up-to-date; `until` is the next review deadline.
    Current {
        /// The review is valid until this timestamp.
        until: DateTime<Utc>,
    },
    /// Review is past due but still within the 30-day grace window.
    ReviewDue {
        /// When the review first became overdue.
        since: DateTime<Utc>,
    },
    /// Review window plus 30-day grace has elapsed.
    Expired {
        /// The timestamp at which the subscription expired.
        expired_at: DateTime<Utc>,
    },
}

fn lock_poisoned() -> IntegrationError {
    IntegrationError::Internal("lock poisoned".into())
}

/// Registry for external compliance-standard subscriptions.
///
/// Maintains per-standard subscriptions (NIST 800-53 Rev.5, DISA STIG,
/// CIS Controls v8, FIPS 140-3, GDPR, HIPAA, ISO 27001, SOC 2, etc.) with
/// versioned revisions, a default 90-day review interval, and a 30-day
/// post-deadline grace window before expiration.
pub struct ExternalStandardRegistry {
    subscriptions: RwLock<HashMap<StandardSubscriptionId, StandardSubscription>>,
    review_history: RwLock<Vec<StandardReviewRecord>>,
    default_review_interval_days: u32,
    emitter: Option<Arc<dyn IntegrationEvidenceEmitter>>,
}

impl ExternalStandardRegistry {
    /// Creates an empty registry with a 90-day default review interval.
    #[must_use]
    pub fn new() -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            review_history: RwLock::new(Vec::new()),
            default_review_interval_days: 90,
            emitter: None,
        }
    }

    /// Creates an empty registry with a custom review interval.
    #[must_use]
    pub fn with_review_interval(days: u32) -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            review_history: RwLock::new(Vec::new()),
            default_review_interval_days: days,
            emitter: None,
        }
    }

    /// Attach an optional [`IntegrationEvidenceEmitter`] for chain-of-custody
    /// evidence emission.
    #[must_use]
    pub fn with_emitter(mut self, emitter: Arc<dyn IntegrationEvidenceEmitter>) -> Self {
        self.emitter = Some(emitter);
        self
    }

    /// Registers a new standard subscription.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the `subscription_id` already exists or a lock is
    /// poisoned.
    #[allow(clippy::unused_async)]
    pub async fn subscribe(
        &self,
        subscription: StandardSubscription,
    ) -> Result<(), IntegrationError> {
        let mut subs = self.subscriptions.write().map_err(|_| lock_poisoned())?;
        if subs.contains_key(&subscription.subscription_id) {
            return Err(IntegrationError::Internal(
                "subscription_id already exists".into(),
            ));
        }
        subs.insert(subscription.subscription_id.clone(), subscription);
        drop(subs);
        Ok(())
    }

    /// Revises a subscription's tracked revision and records a review entry.
    ///
    /// The previous revision is recorded in the review history and the
    /// `next_review_due_at` timestamp is advanced by the default interval.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the subscription is unknown or a lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn revise(
        &self,
        subscription_id: &StandardSubscriptionId,
        new_revision: String,
        reviewer: String,
        note: String,
    ) -> Result<(), IntegrationError> {
        let record = {
            let mut subs = self.subscriptions.write().map_err(|_| lock_poisoned())?;
            let sub = subs
                .get_mut(subscription_id)
                .ok_or_else(|| IntegrationError::Internal("unknown subscription".into()))?;

            let record = StandardReviewRecord {
                subscription_id: subscription_id.clone(),
                reviewed_at: Utc::now(),
                reviewer,
                revision_before: sub.current_revision.clone(),
                revision_after: new_revision.clone(),
                note,
            };

            sub.current_revision = new_revision;
            sub.last_reviewed_at = record.reviewed_at;
            sub.next_review_due_at =
                record.reviewed_at + Duration::days(i64::from(self.default_review_interval_days));
            drop(subs);
            record
        };

        self.review_history
            .write()
            .map_err(|_| lock_poisoned())?
            .push(record);

        if let Some(ref emitter) = self.emitter {
            let sub_for_emit = self
                .subscriptions
                .read()
                .map_err(|_| lock_poisoned())?
                .get(subscription_id)
                .cloned();
            if let Some(sub) = sub_for_emit {
                let _ = emitter
                    .emit_standard_update_available(&sub, &sub.current_revision)
                    .await;
            }
        }

        Ok(())
    }

    /// Returns the subscription status relative to `now`.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the subscription is unknown.
    #[allow(clippy::unused_async)]
    pub async fn status(
        &self,
        subscription_id: &StandardSubscriptionId,
        now: DateTime<Utc>,
    ) -> Result<SubscriptionStatus, IntegrationError> {
        let subs = self.subscriptions.read().map_err(|_| lock_poisoned())?;
        let sub = subs
            .get(subscription_id)
            .ok_or_else(|| IntegrationError::Internal("unknown subscription".into()))?;

        let grace_deadline = sub.next_review_due_at + Duration::days(30);
        let status = if now > grace_deadline {
            Ok(SubscriptionStatus::Expired {
                expired_at: grace_deadline,
            })
        } else if now > sub.next_review_due_at {
            Ok(SubscriptionStatus::ReviewDue {
                since: sub.next_review_due_at,
            })
        } else {
            Ok(SubscriptionStatus::Current {
                until: sub.next_review_due_at,
            })
        };
        drop(subs);
        status
    }

    /// Lists all registered subscriptions.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_subscriptions(&self) -> Vec<StandardSubscription> {
        self.subscriptions
            .read()
            .ok()
            .map(|s| s.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Lists subscriptions filtered by standard kind.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_by_kind(&self, kind: StandardKind) -> Vec<StandardSubscription> {
        self.subscriptions
            .read()
            .ok()
            .map(|s| {
                s.values()
                    .filter(|sub| sub.standard == kind)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns subscription IDs that are past their review deadline.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_due_for_review(&self, now: DateTime<Utc>) -> Vec<StandardSubscriptionId> {
        self.subscriptions
            .read()
            .ok()
            .map(|s| {
                s.values()
                    .filter(|sub| now > sub.next_review_due_at)
                    .map(|sub| sub.subscription_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns subscription IDs past the 30-day grace window.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_expired(&self, now: DateTime<Utc>) -> Vec<StandardSubscriptionId> {
        self.subscriptions
            .read()
            .ok()
            .map(|s| {
                s.values()
                    .filter(|sub| {
                        let grace_deadline = sub.next_review_due_at + Duration::days(30);
                        now > grace_deadline
                    })
                    .map(|sub| sub.subscription_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the review history for a specific subscription.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn review_history_for(
        &self,
        subscription_id: &StandardSubscriptionId,
    ) -> Vec<StandardReviewRecord> {
        self.review_history
            .read()
            .ok()
            .map(|h| {
                h.iter()
                    .filter(|r| &r.subscription_id == subscription_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Removes a subscription from the registry.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the subscription is unknown or a lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn unsubscribe(
        &self,
        subscription_id: &StandardSubscriptionId,
    ) -> Result<(), IntegrationError> {
        let mut subs = self.subscriptions.write().map_err(|_| lock_poisoned())?;
        if subs.remove(subscription_id).is_none() {
            return Err(IntegrationError::Internal("unknown subscription".into()));
        }
        drop(subs);
        Ok(())
    }
}

impl Default for ExternalStandardRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps a [`StandardKind`] to its canonical public URL.
#[must_use]
pub const fn standard_kind_to_canonical_url(kind: StandardKind) -> &'static str {
    match kind {
        StandardKind::Nist80053Rev5 => {
            "https://csrc.nist.gov/pubs/sp/800/53/r5/upd1/final"
        }
        StandardKind::NistSp800218Ssdf => {
            "https://csrc.nist.gov/projects/ssdf"
        }
        StandardKind::NistSp800207ZeroTrust => {
            "https://csrc.nist.gov/pubs/sp/800/207/final"
        }
        StandardKind::NistSp800193Firmware => {
            "https://csrc.nist.gov/pubs/sp/800/193/final"
        }
        StandardKind::DisaStig => "https://public.cyber.mil/stigs/",
        StandardKind::CisControlsV8 => "https://www.cisecurity.org/controls/v8",
        StandardKind::Fips1403 => {
            "https://csrc.nist.gov/projects/cryptographic-module-validation-program/fips-140-3-standards"
        }
        StandardKind::Gdpr => "https://gdpr-info.eu/",
        StandardKind::Hipaa => "https://www.hhs.gov/hipaa/",
        StandardKind::Iso27001 => "https://www.iso.org/standard/27001",
        StandardKind::Soc2 => {
            "https://www.aicpa-cima.com/topic/audit-assurance/soc-suite-of-services"
        }
    }
}
