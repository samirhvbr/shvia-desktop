//! A local Code agent belongs to the server-verified account and origin that opened it.
//! A new document must prove that identity before it can use the window's agents.
use super::{js_str, reply, Sidecars, WebviewWindow};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::Read, sync::Mutex, time::{Duration, Instant}};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Page {
    origin: String,
    nonce: String,
    epoch: u64,
}

#[derive(Clone)]
pub(super) struct Grant {
    page: Page,
    pub(super) scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Owner {
    origin: String,
    id: u64,
}

/// A Pentest proof is independent of Code permissions and belongs to one page generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PentestAccount {
    page: Page,
    pub(super) id: u64,
}

impl PentestAccount {
    pub(super) fn origin(&self) -> &str {
        &self.page.origin
    }
}

struct PentestProof {
    token: [u8; 32],
    until: Instant,
}

#[derive(Default)]
struct WindowOwner {
    page: Option<Page>,
    awaiting_finish: bool,
    document_owner: Option<Owner>,
    owner: Option<(Owner, String)>,
    bound: bool,
    binding: bool,
    snapshot: Option<serde_json::Value>,
    pentest_account: Option<u64>,
    pentest_proof: Option<PentestProof>,
    token_hashes: Vec<[u8; 32]>,
    epoch: u64,
}

#[derive(Default)]
pub(crate) struct ResumeOwners(Mutex<HashMap<String, WindowOwner>>);

impl ResumeOwners {
    pub(crate) fn started(&self, label: &str) {
        let Ok(mut windows) = self.0.lock() else {
            return;
        };
        let w = windows.entry(label.to_owned()).or_default();
        w.awaiting_finish = true;
        w.epoch += 1;
        if let Some(p) = w.page.as_mut() {
            // A canceled download can leave the original document visible. It can
            // prove itself again, but replies from before navigation are obsolete.
            p.epoch = w.epoch;
        }
        w.bound = false;
        w.binding = false;
        w.token_hashes.clear();
        w.pentest_proof = None;
    }

    pub(crate) fn finished(&self, label: &str, origin: String) -> Option<String> {
        let mut windows = self.0.lock().ok()?;
        let w = windows.entry(label.to_owned()).or_default();
        // WebView2 can report NavigationCompleted for a canceled navigation
        // without first reporting ContentLoading. The existing shim still
        // belongs to this document; do not rotate its nonce or inject it again.
        if !w.awaiting_finish && w.page.is_some() {
            return None;
        }
        w.awaiting_finish = false;
        w.epoch += 1;
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        w.page = Some(Page {
            origin,
            nonce: nonce.clone(),
            epoch: w.epoch,
        });
        w.document_owner = None;
        w.pentest_account = None;
        w.pentest_proof = None;
        w.bound = false;
        w.binding = false;
        w.token_hashes.clear();
        Some(nonce)
    }

    pub(crate) fn forget(&self, label: &str) {
        if let Ok(mut windows) = self.0.lock() {
            windows.remove(label);
        }
    }

    fn begin(&self, label: &str, nonce: &str, origin: &str) -> Option<Page> {
        let mut windows = self.0.lock().ok()?;
        let w = windows.get_mut(label)?;
        let page = w
            .page
            .as_ref()
            .filter(|p| p.nonce == nonce && p.origin == origin)?;
        if w.binding {
            return None;
        }
        w.binding = true;
        Some(page.clone())
    }

    /// Updates the proof only if its document still owns this request. Clear the
    /// previous account's sessions before the new one can list or address them.
    fn complete(
        &self,
        label: &str,
        page: &Page,
        id: Option<u64>,
        token: &str,
        clear: impl FnOnce(),
    ) -> Option<String> {
        let mut windows = self.0.lock().ok()?;
        let w = windows.get_mut(label)?;
        if w.page.as_ref() != Some(page) || !w.binding {
            return None;
        }
        w.binding = false;
        // Failure to verify a *new* key does not replace a previous proof of
        // this document. A successful proof of another owner revokes it below.
        let owner = Owner {
            origin: page.origin.clone(),
            id: id?,
        };
        if w.document_owner
            .as_ref()
            .is_some_and(|previous| previous != &owner)
        {
            // A shared cookie jar can change while the old document is still
            // alive. Its in-memory state cannot become another account's.
            clear();
            *w = WindowOwner::default();
            return None;
        }
        w.document_owner = Some(owner.clone());
        if w.owner
            .as_ref()
            .is_some_and(|(previous, _)| previous != &owner)
        {
            clear();
            w.owner = None;
            w.snapshot = None;
        }
        let (_, scope) = w
            .owner
            .get_or_insert_with(|| (owner, uuid::Uuid::new_v4().simple().to_string()));
        let hash = Self::fingerprint(token);
        if !w.token_hashes.contains(&hash) {
            // A send already queued with the previous verified key may finish.
            if w.token_hashes.len() == 4 {
                w.token_hashes.remove(0);
            }
            w.token_hashes.push(hash);
        }
        w.bound = true;
        Some(scope.clone())
    }

    fn pentest_document(&self, label: &str, nonce: &str, origin: &str, token: &[u8; 32]) -> Option<(Page, Option<u64>)> {
        let windows = self.0.lock().ok()?;
        let w = windows.get(label)?;
        let page = w.page.as_ref().filter(|p| p.nonce == nonce && p.origin == origin)?;
        let cached = w.pentest_proof.as_ref()
            .filter(|proof| proof.until > Instant::now() && &proof.token == token)
            .and(w.pentest_account);
        Some((page.clone(), cached))
    }

    fn confirm_pentest(&self, label: &str, page: &Page, id: u64, token: [u8; 32], duration: Duration) -> Option<()> {
        let mut windows = self.0.lock().ok()?;
        let w = windows.get_mut(label)?;
        if w.page.as_ref() != Some(page) {
            return None;
        }
        if w.pentest_account.is_some_and(|owner| owner != id) {
            w.pentest_proof = None;
            return None;
        }
        w.pentest_account = Some(id);
        w.pentest_proof = Some(PentestProof { token, until: Instant::now() + duration });
        Some(())
    }

    fn pentest_current(&self, label: &str, account: &PentestAccount) -> bool {
        self.0.lock().ok().is_some_and(|windows| windows.get(label).is_some_and(|w| {
            w.page.as_ref() == Some(&account.page)
                && w.pentest_account == Some(account.id)
                && w.pentest_proof.as_ref().is_some_and(|p| p.until > Instant::now())
        }))
    }

    pub(super) fn grant(&self, label: &str, nonce: &str, origin: &str) -> Option<Grant> {
        let windows = self.0.lock().ok()?;
        let w = windows.get(label)?;
        if !w.bound
            || !w
                .page
                .as_ref()
                .is_some_and(|p| p.nonce == nonce && p.origin == origin)
        {
            return None;
        }
        let (_, scope) = w.owner.as_ref()?;
        Some(Grant {
            page: w.page.clone()?,
            scope: scope.clone(),
        })
    }

    fn matches(w: &WindowOwner, grant: &Grant) -> bool {
        w.bound
            && w.page.as_ref() == Some(&grant.page)
            && w.owner
                .as_ref()
                .is_some_and(|(_, scope)| scope == &grant.scope)
    }

    fn fingerprint(token: &str) -> [u8; 32] {
        Sha256::digest(token.as_bytes()).into()
    }

    pub(super) fn allows_token(&self, label: &str, grant: &Grant, token: &str) -> bool {
        if token.is_empty() || token.len() > 512 {
            return false;
        }
        self.0.lock().ok().is_some_and(|windows| {
            windows.get(label).is_some_and(|w| {
                Self::matches(w, grant) && w.token_hashes.contains(&Self::fingerprint(token))
            })
        })
    }

    pub(super) fn save_snapshot(
        &self,
        label: &str,
        grant: &Grant,
        value: Option<&serde_json::Value>,
    ) -> bool {
        let Some(value) = value.filter(|v| v.is_object()) else {
            return false;
        };
        if value.to_string().len() > 4 * 1024 * 1024 {
            return false;
        }
        let Ok(mut windows) = self.0.lock() else {
            return false;
        };
        let Some(w) = windows.get_mut(label).filter(|w| Self::matches(w, grant)) else {
            return false;
        };
        w.snapshot = Some(value.clone());
        true
    }

    pub(super) fn snapshot(&self, label: &str, grant: &Grant) -> Option<serde_json::Value> {
        self.0
            .lock()
            .ok()?
            .get(label)
            .filter(|w| Self::matches(w, grant))?
            .snapshot
            .clone()
    }

    fn delivery(&self, label: &str, scope: &str) -> Option<Page> {
        let windows = self.0.lock().ok()?;
        let w = windows.get(label)?;
        if !w.bound || !w.owner.as_ref().is_some_and(|(_, actual)| actual == scope) {
            return None;
        }
        w.page.clone()
    }

    fn clear(&self, label: &str, nonce: &str, origin: &str, clear: impl FnOnce()) -> bool {
        let Ok(mut windows) = self.0.lock() else {
            return false;
        };
        let Some(w) = windows.get_mut(label) else {
            return false;
        };
        if !w
            .page
            .as_ref()
            .is_some_and(|p| p.nonce == nonce && p.origin == origin)
        {
            return false;
        }
        // Signing out revokes the entire window, like closing it. A trusted
        // current document can do this before proof finishes, but cannot list,
        // attach, send to or kill a named agent without a verified grant.
        *w = WindowOwner::default();
        clear();
        true
    }
}

pub(crate) fn origin(url: &tauri::Url) -> Option<String> {
    if !crate::is_server_origin(url) {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

pub(super) fn page_origin(window: &WebviewWindow) -> Option<String> {
    origin(&window.url().ok()?)
}

fn account_client() -> Option<reqwest::blocking::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()
}

fn account_response(request: reqwest::blocking::RequestBuilder) -> Option<serde_json::Value> {
    let response = request.send().ok()?;
    if response.status() != reqwest::StatusCode::OK {
        return None;
    }
    let mut body = Vec::new();
    response.take(1024).read_to_end(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn verify(origin: &str, token: &str) -> Option<u64> {
    let request = account_client()?.get(format!("{origin}/api/v1/code/desktop-owner"))
        .header("Accept", "application/json")
        .bearer_auth(token);
    account_response(request)?.get("id")?.as_u64().filter(|id| *id > 0)
}

pub(super) const PENTEST_OWNER_REQUIRED: &str = "Não foi possível verificar a conta do Pentest. Recarregue a página e tente novamente.";

pub(super) fn pentest_account(window: &WebviewWindow, req: &str, token: &str, grant: Option<&str>) -> Option<PentestAccount> {
    if token.is_empty() || token.len() > 512 {
        return None;
    }
    let (nonce, _) = req.split_once("-r")?;
    let origin = page_origin(window)?;
    let owners = window.state::<ResumeOwners>();
    let hash = ResumeOwners::fingerprint(token);
    let (page, cached) = owners.pentest_document(window.label(), nonce, &origin, &hash)?;
    let id = if let Some(id) = cached.filter(|_| grant.is_none()) {
        id
    } else {
        let mut request = account_client()?
            .post(format!("{origin}/api/v1/pentest/desktop-owner"))
            .header("Accept", "application/json")
            .bearer_auth(token);
        if let Some(grant) = grant {
            request = request.header("Content-Type", "application/json")
                .body(json!({"grant": grant}).to_string());
        }
        let response = account_response(request)?;
        let id = response.get("id").and_then(serde_json::Value::as_u64).filter(|id| *id > 0)?;
        let seconds = response.get("expires_in").and_then(serde_json::Value::as_u64)?.min(120);
        if seconds == 0 {
            return None;
        }
        owners.confirm_pentest(window.label(), &page, id, hash, Duration::from_secs(seconds))?;
        id
    };
    let account = PentestAccount { page, id };
    pentest_current(window, &account).then_some(account)
}

pub(super) fn pentest_current(window: &WebviewWindow, account: &PentestAccount) -> bool {
    page_origin(window).as_deref() == Some(account.origin()) && pentest_document_current(window, account)
}

// Safe inside a native state lock: unlike page_origin, this never asks the WebView's UI thread.
pub(super) fn pentest_document_current(window: &WebviewWindow, account: &PentestAccount) -> bool {
    window.state::<ResumeOwners>().pentest_current(window.label(), account)
}

pub(super) fn bind(window: &WebviewWindow, req: &str, nonce: &str, origin: &str, token: &str) {
    let owners = window.state::<ResumeOwners>();
    let Some(page) = owners.begin(window.label(), nonce, origin) else {
        return refused(window, req);
    };
    let token = token.to_owned();
    let w = window.clone();
    let req = req.to_owned();
    std::thread::spawn(move || {
        let id = if token.is_empty() || token.len() > 512 {
            None
        } else {
            verify(&page.origin, &token)
        };
        let app = w.app_handle().clone();
        let _ = app.run_on_main_thread(move || {
            if page_origin(&w).as_deref() != Some(page.origin.as_str()) {
                return;
            }
            let scope = w
                .state::<ResumeOwners>()
                .complete(w.label(), &page, id, &token, || {
                    w.state::<Sidecars>().kill_one(w.label());
                });
            match scope {
                Some(scope) => reply(&w, &req, true, json!({ "scope": scope })),
                None => refused(&w, &req),
            }
        });
    });
}

pub(super) fn unbind(window: &WebviewWindow, req: &str, nonce: &str, origin: &str) {
    if window
        .state::<ResumeOwners>()
        .clear(window.label(), nonce, origin, || {
            window.state::<Sidecars>().kill_one(window.label());
        })
    {
        reply(window, req, true, json!({}));
    } else {
        refused(window, req);
    }
}

pub(super) fn refused(window: &WebviewWindow, req: &str) {
    reply(
        window,
        req,
        false,
        json!({ "codigo": "desktop_owner_required", "error": "Não foi possível verificar a conta do Code. Recarregue a página e tente novamente." }),
    );
}

pub(super) fn in_document(nonce: &str, script: &str) -> String {
    format!(
        "if(window.__shviaCodeDocument==={}){{{script}}}",
        js_str(nonce)
    )
}

pub(super) fn deliver(window: &WebviewWindow, scope: &str, script: String) {
    let Some(page) = window
        .state::<ResumeOwners>()
        .delivery(window.label(), scope)
    else {
        return;
    };
    let w = window.clone();
    let scope = scope.to_owned();
    let _ = window.app_handle().run_on_main_thread(move || {
        if w.state::<ResumeOwners>()
            .delivery(w.label(), &scope)
            .as_ref()
            == Some(&page)
            && page_origin(&w).as_deref() == Some(page.origin.as_str())
        {
            // Eval can execute after another navigation. Its payload is only
            // evaluated in the original document, even on the same host.
            let _ = w.eval(in_document(&page.nonce, &script));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pentest_proof_cannot_cross_a_document_or_switch_accounts_in_place() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("main", origin.into()).unwrap();
        let hash = ResumeOwners::fingerprint("fixture");
        let (old, cached) = owners.pentest_document("main", &nonce, origin, &hash).unwrap();
        assert_eq!(cached, None);
        owners.confirm_pentest("main", &old, 7, hash, Duration::from_secs(1)).unwrap();
        let proof = PentestAccount { page: old.clone(), id: 7 };
        assert!(owners.pentest_current("main", &proof));
        assert!(owners.grant("main", &nonce, origin).is_none());
        assert!(owners.confirm_pentest("main", &old, 9, hash, Duration::from_secs(1)).is_none());
        assert!(!owners.pentest_current("main", &proof));
        owners.started("main");
        let new_nonce = owners.finished("main", origin.into()).unwrap();
        assert!(owners.confirm_pentest("main", &old, 7, hash, Duration::from_secs(1)).is_none());
        let (new, cached) = owners.pentest_document("main", &new_nonce, origin, &hash).unwrap();
        assert_eq!(cached, None);
        assert!(owners.confirm_pentest("main", &new, 9, hash, Duration::from_secs(1)).is_some());
    }

    #[test]
    fn resume_requires_a_new_proof_and_keeps_only_the_same_account_and_origin() {
        let owners = ResumeOwners::default();
        let a = "https://ai.shvia.org";
        let b = "https://ia.shvia.org";
        let finish = |origin: &str| owners.finished("window", origin.into()).unwrap();
        let bind = |nonce: &str, origin: &str, id, clear: &mut bool| {
            let page = owners.begin("window", nonce, origin).unwrap();
            owners
                .complete("window", &page, Some(id), "code-test", || *clear = true)
                .unwrap()
        };
        let mut clear = false;
        let first = finish(a);
        let scope = bind(&first, a, 7, &mut clear);
        let old_grant = owners.grant("window", &first, a).unwrap();
        assert!(owners.save_snapshot("window", &old_grant, Some(&json!({ "text": "account A" }))));
        owners.started("window");
        assert!(owners.snapshot("window", &old_grant).is_none());
        assert!(owners.grant("window", &first, a).is_none());
        assert!(owners.delivery("window", &scope).is_none());
        let second = finish(a);
        assert!(owners.begin("window", &first, a).is_none());
        assert_eq!(bind(&second, a, 7, &mut clear), scope);
        let reloaded_grant = owners.grant("window", &second, a).unwrap();
        assert_eq!(
            owners.snapshot("window", &reloaded_grant),
            Some(json!({ "text": "account A" }))
        );
        assert!(!owners.save_snapshot(
            "window",
            &old_grant,
            Some(&json!({ "text": "stale document" }))
        ));
        assert!(!clear);
        owners.started("window");
        let third = finish(a);
        assert_ne!(bind(&third, a, 8, &mut clear), scope);
        assert!(!owners.save_snapshot(
            "window",
            &reloaded_grant,
            Some(&json!({ "text": "wrong account" }))
        ));
        assert!(owners
            .snapshot("window", &owners.grant("window", &third, a).unwrap())
            .is_none());
        assert!(clear);
        assert!(owners.delivery("window", &scope).is_none());
        assert!(owners.snapshot("window", &old_grant).is_none());

        clear = false;
        owners.started("window");
        let fourth = finish(b);
        assert!(owners.begin("window", &fourth, a).is_none());
        bind(&fourth, b, 8, &mut clear);
        assert!(clear);
    }

    #[test]
    fn a_completion_without_a_load_start_does_not_replace_the_live_document() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("window", origin.into()).unwrap();
        let page = owners.begin("window", &nonce, origin).unwrap();
        let scope = owners
            .complete("window", &page, Some(7), "code-test", || panic!())
            .unwrap();

        assert!(owners.finished("window", origin.into()).is_none());
        assert!(owners
            .finished("window", "https://ia.shvia.org".into())
            .is_none());
        assert_eq!(owners.delivery("window", &scope), Some(page));
        assert!(owners.grant("window", &nonce, origin).is_some());

        owners.started("window");
        let new_nonce = owners.finished("window", origin.into()).unwrap();
        assert_ne!(new_nonce, nonce);
        assert!(owners.grant("window", &nonce, origin).is_none());
        assert!(owners.grant("window", &new_nonce, origin).is_none());
        assert!(owners.finished("window", origin.into()).is_none());
    }

    #[test]
    fn a_late_proof_cannot_bind_after_navigation_or_logout() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("window", origin.into()).unwrap();
        let stale = owners.begin("window", &nonce, origin).unwrap();
        owners.started("window");
        assert!(owners
            .complete("window", &stale, Some(7), "code-test", || panic!())
            .is_none());
        let current = owners.begin("window", &nonce, origin).unwrap();
        assert!(owners.clear("window", &nonce, origin, || {}));
        assert!(owners
            .complete("window", &current, Some(7), "code-test", || panic!())
            .is_none());
        assert!(owners.begin("window", &nonce, origin).is_none());
    }

    #[test]
    fn refused_proof_never_authorizes_any_operation() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("window", origin.into()).unwrap();
        let page = owners.begin("window", &nonce, origin).unwrap();
        assert!(owners
            .complete("window", &page, None, "code-invalid", || panic!())
            .is_none());
        assert!(owners.grant("window", &nonce, origin).is_none());
        assert!(owners.begin("window", &nonce, origin).is_some());
    }

    #[test]
    fn a_different_account_cannot_reuse_the_same_document_after_a_canceled_navigation() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("window", origin.into()).unwrap();
        let initial = owners.begin("window", &nonce, origin).unwrap();
        let scope = owners
            .complete("window", &initial, Some(7), "code-test", || panic!())
            .unwrap();
        owners.started("window");
        let retry = owners.begin("window", &nonce, origin).unwrap();
        let mut cleared = false;
        assert!(owners
            .complete("window", &retry, Some(8), "code-test-B", || cleared = true)
            .is_none());
        assert!(cleared);
        assert!(owners.delivery("window", &scope).is_none());
        assert!(owners.begin("window", &nonce, origin).is_none());
    }

    #[test]
    fn only_keys_verified_for_this_document_can_reach_an_agent() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let nonce = owners.finished("window", origin.into()).unwrap();
        let first = owners.begin("window", &nonce, origin).unwrap();
        let scope = owners
            .complete("window", &first, Some(7), "A-initial", || panic!())
            .unwrap();
        let grant = owners.grant("window", &nonce, origin).unwrap();
        assert!(owners.allows_token("window", &grant, "A-initial"));
        assert!(!owners.allows_token("window", &grant, "A-renewed"));
        let renew = owners.begin("window", &nonce, origin).unwrap();
        assert_eq!(
            owners.complete("window", &renew, Some(7), "A-renewed", || panic!()),
            Some(scope)
        );
        assert!(owners.allows_token("window", &grant, "A-renewed"));
        assert!(!owners.allows_token("window", &grant, "B-renewed"));
        let failed = owners.begin("window", &nonce, origin).unwrap();
        assert!(owners
            .complete("window", &failed, None, "unverified", || panic!())
            .is_none());
        assert!(owners.allows_token("window", &grant, "A-renewed"));
        let mut cleared = false;
        let switched = owners.begin("window", &nonce, origin).unwrap();
        assert!(owners
            .complete("window", &switched, Some(8), "B-renewed", || cleared = true)
            .is_none());
        assert!(cleared);
        assert!(!owners.allows_token("window", &grant, "A-renewed"));
        assert!(!owners.allows_token("window", &grant, "B-renewed"));
    }

    #[test]
    fn logout_clears_the_detached_owner_and_snapshot_even_with_a_proof_pending() {
        let owners = ResumeOwners::default();
        let origin = "https://ai.shvia.org";
        let first = owners.finished("window", origin.into()).unwrap();
        let proof = owners.begin("window", &first, origin).unwrap();
        let old_scope = owners
            .complete("window", &proof, Some(7), "account-A", || panic!())
            .unwrap();
        let old_grant = owners.grant("window", &first, origin).unwrap();
        assert!(owners.save_snapshot("window", &old_grant, Some(&json!({"private":"account A"}))));

        owners.started("window");
        let new_page = owners.finished("window", origin.into()).unwrap();
        let pending = owners.begin("window", &new_page, origin).unwrap();
        assert!(!owners.clear("window", &first, origin, || panic!()));
        assert!(!owners.clear("window", &new_page, "https://ia.shvia.org", || panic!()));
        let mut cleared = false;
        assert!(owners.clear("window", &new_page, origin, || cleared = true));
        assert!(cleared);
        assert!(owners
            .complete("window", &pending, Some(7), "account-A", || panic!())
            .is_none());

        let reloaded = owners.finished("window", origin.into()).unwrap();
        let proof = owners.begin("window", &reloaded, origin).unwrap();
        assert_ne!(
            owners.complete("window", &proof, Some(7), "account-A", || panic!()),
            Some(old_scope)
        );
        assert!(owners
            .snapshot(
                "window",
                &owners.grant("window", &reloaded, origin).unwrap()
            )
            .is_none());
    }

    #[test]
    fn owner_verification_never_follows_a_redirect_with_the_bearer() {
        use std::{io::Write, net::TcpListener};
        let redirect = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let target = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let origin = format!("http://{}", redirect.local_addr().unwrap());
        let location = format!("http://{}", target.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = redirect.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 2048 {
                let mut byte = [0];
                if stream.read(&mut byte).unwrap() != 1 {
                    break;
                }
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /api/v1/code/desktop-owner http/1.1"));
            assert!(request.contains("authorization: bearer test-proof"));
            write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });

        assert_eq!(verify(&origin, "test-proof"), None);
        server.join().unwrap();
        target.set_nonblocking(true).unwrap();
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
