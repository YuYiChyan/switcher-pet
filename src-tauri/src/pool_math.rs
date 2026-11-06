//! Pure, approximate Plus-equivalent accounting. No network or account secrets.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn yes() -> bool { true }
fn model_minutes() -> u32 { 360 }
fn balance_minutes() -> u32 { 30 }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcePolicy {
    pub included: bool,
    pub short_weight: Option<f64>,
    pub weekly_weight: Option<f64>,
    /// auto | limited | none; explicit user override, not a provider claim.
    pub short_window: String,
    pub billing_group: String,
    pub probe_models: bool,
    /// off | openai_usage | new_api_budget. Never guess a credential endpoint.
    pub balance_adapter: String,
    pub balance_reference: Option<f64>,
    pub model_minutes: u32,
    pub balance_minutes: u32,
}
impl Default for SourcePolicy {
    fn default() -> Self { Self { included: yes(), short_weight: None, weekly_weight: None,
        short_window: "auto".into(), billing_group: String::new(), probe_models: false,
        balance_adapter: "off".into(), balance_reference:None, model_minutes: model_minutes(), balance_minutes: balance_minutes() } }
}
impl SourcePolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.balance_reference.is_some_and(|v|!v.is_finite() || v<=0.0 || v>1e15){return Err("Balance reference must be positive and finite".into());}
        for w in [self.short_weight, self.weekly_weight].into_iter().flatten() {
            if !w.is_finite() || !(0.01..=10000.0).contains(&w) { return Err("Weight must be between 0.01 and 10000".into()); }
        }
        if !["auto", "limited", "none"].contains(&self.short_window.as_str()) { return Err("Invalid short-window setting".into()); }
        if !["off", "auto", "deepseek", "openai_usage", "new_api_budget"].contains(&self.balance_adapter.as_str()) { return Err("Unsupported read-only balance adapter".into()); }
        if !(1..=10080).contains(&self.model_minutes) || !(1..=10080).contains(&self.balance_minutes) { return Err("Probe interval must be 1–10080 minutes".into()); }
        if self.billing_group.len() > 128 { return Err("Billing group is too long".into()); }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PoolSettings { pub sources: BTreeMap<String, SourcePolicy> }

pub fn default_weight(plan: &str) -> f64 {
    if plan.to_lowercase().contains("pro") { 10.0 } else { 1.0 }
}
/// A workspace can contain many separately metered Business seats. Deduplicate
/// reimports of one seat, never all users sharing the workspace/account ID.
pub fn subscription_identity(workspace:Option<&str>,user:Option<&str>,fallback:&str)->String {
    format!("openai:{}:{}",workspace.unwrap_or(""),user.filter(|s|!s.is_empty()).unwrap_or(fallback))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance { pub amount: f64, pub unit: String, pub kind: String, pub updated_at: i64, #[serde(default)] pub capacity: Option<f64>, #[serde(default)] pub estimated:bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMember {
    pub id: String,
    pub name: String,
    pub subscription: bool,
    /// Internal dedup key; not returned by the UI adapter.
    pub identity: String,
    pub plan: String,
    pub policy: SourcePolicy,
    pub short_limited: Option<bool>,
    pub short_left: Option<f64>,
    #[serde(default)] pub weekly_supported: Option<bool>,
    pub weekly_left: Option<f64>,
    pub updated_at: Option<i64>,
    pub balance: Option<Balance>,
    pub available_models: Vec<String>,
    pub unavailable: bool,
}
#[derive(Debug, Default, Serialize)]
pub struct WindowTotal { pub remaining: f64, pub capacity: f64, pub known: usize, pub unknown: usize, pub stale: usize }
#[derive(Debug, Default, Serialize)]
pub struct PoolTotal {
    pub members: usize, pub subscription_members: usize, pub duplicates: usize,
    pub show_short: bool, pub short_uncertain: bool,
    pub short: WindowTotal, pub weekly: WindowTotal,
    pub balances: BTreeMap<String, f64>, pub balance_unknown: usize,
    pub balance_capacities: BTreeMap<String, f64>,
    pub balance_estimated: BTreeMap<String, bool>,
    pub api_sources: usize, pub api_models: usize, pub unavailable: usize,
}
fn fresh(t: Option<i64>, now: i64) -> bool { t.is_some_and(|t| t <= now + 60 && now - t <= 1800) }
fn percent(v: Option<f64>) -> Option<f64> { v.filter(|v| v.is_finite() && (0.0..=100.0).contains(v)) }
/// Exact model scope: another API's money must never appear under this model.
pub fn members_for_model(members:&[PoolMember], model:&str)->Vec<PoolMember> {
    members.iter().filter(|m|!model.is_empty() && !m.unavailable &&
        (m.available_models.iter().any(|v|v==model) ||
         (m.subscription && model.starts_with("gpt-") && !model.starts_with("gpt-oss-"))))
        .cloned().collect()
}
pub fn aggregate(members: &[PoolMember], now: i64) -> PoolTotal {
    let mut out = PoolTotal::default();
    let mut subscriptions = BTreeMap::<String, &PoolMember>::new();
    let mut balances = BTreeMap::<String, Option<&Balance>>::new();
    let mut models = BTreeSet::new();
    let mut incomplete_capacity = BTreeSet::new();
    for m in members.iter().filter(|m| m.policy.included) {
        out.members += 1;
        out.unavailable += usize::from(m.unavailable);
        if m.subscription {
            let key = if m.policy.billing_group.is_empty() { &m.identity } else { &m.policy.billing_group };
            match subscriptions.get(key) {
                Some(old) => { out.duplicates += 1; if m.updated_at > old.updated_at { subscriptions.insert(key.clone(), m); } }
                None => { subscriptions.insert(key.clone(), m); }
            }
        } else {
            if !m.unavailable && !m.available_models.is_empty() {
                out.api_sources += 1;
                models.extend(m.available_models.iter().cloned());
            }
            let group = if m.policy.billing_group.is_empty() { &m.identity } else { &m.policy.billing_group };
            // Choose one latest observation per billing group. Multiple API keys
            // must not inflate a shared balance. Currency never converted implicitly.
            let entry = balances.entry(group.clone()).or_default();
            if let Some(balance) = &m.balance {
                if entry.is_none_or(|old| balance.updated_at > old.updated_at) { *entry = Some(balance); }
            }
        }
    }
    out.api_models = models.len();
    out.subscription_members = subscriptions.len();
    for m in subscriptions.values() {
        let is_fresh = fresh(m.updated_at, now);
        let limited = match m.policy.short_window.as_str() {
            "limited" => Some(true), "none" => Some(false), _ => m.short_limited,
        };
        if limited != Some(false) { out.show_short = true; }
        if limited.is_none() { out.short_uncertain = true; out.short.unknown += 1;
            out.short.capacity += 100.0*m.policy.short_weight.unwrap_or_else(||default_weight(&m.plan)); }
        if limited == Some(true) {
            let weight = m.policy.short_weight.unwrap_or_else(|| default_weight(&m.plan));
            out.short.capacity += 100.0 * weight;
            if let Some(left) = m.short_left.and_then(|v| percent(Some(v))) {
                out.short.remaining += weight * left; out.short.known += 1;
                if !is_fresh {out.short.stale += 1;}
            } else { out.short.unknown += 1; }
        }
        if m.weekly_supported==Some(false) {continue;}
        let weight = m.policy.weekly_weight.unwrap_or_else(|| default_weight(&m.plan));
        out.weekly.capacity += 100.0 * weight;
        if let Some(left) = m.weekly_left.and_then(|v| percent(Some(v))) {
            out.weekly.remaining += weight * left; out.weekly.known += 1;
            if !is_fresh {out.weekly.stale += 1;}
        } else { out.weekly.unknown += 1; }
    }
    for balance in balances.values() {
        match balance.filter(|b| b.amount.is_finite() && fresh(Some(b.updated_at), now)) {
            Some(b) => {
                let (unit, amount) = if b.unit == "USDcent" { ("USD", b.amount / 100.0) } else { (b.unit.as_str(), b.amount) };
                let key=format!("{} / {}", b.kind, unit);
                *out.balance_estimated.entry(key.clone()).or_default() |= b.estimated;
                *out.balances.entry(key.clone()).or_default() += amount;
                if let Some(capacity)=b.capacity.filter(|v|v.is_finite() && *v>0.0) {
                    *out.balance_capacities.entry(key).or_default() += if b.unit=="USDcent" {capacity/100.0} else {capacity};
                } else { incomplete_capacity.insert(key); }
            }
            None => out.balance_unknown += 1,
        }
    }
    for key in incomplete_capacity { out.balance_capacities.remove(&key); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sub(id: &str, plan: &str, short: Option<bool>) -> PoolMember {
        PoolMember { id:id.into(), name:id.into(), identity:id.into(), subscription:true, plan:plan.into(), policy:SourcePolicy::default(), short_limited:short, short_left:Some(50.0), weekly_supported:Some(true), weekly_left:Some(50.0), updated_at:Some(1000), balance:None, available_models:vec![], unavailable:false }
    }
    #[test] fn editable_estimates_exceed_100() {
        let mut p=sub("p","pro",Some(false)); p.policy.weekly_weight=Some(20.0);
        let t=aggregate(&[sub("a","plus",Some(true)),p],1000);
        assert_eq!(t.weekly.remaining,1050.0); assert_eq!(t.short.remaining,50.0); assert_eq!(t.short.capacity,100.0);
    }
    #[test] fn default_pro_is_10x_not_an_official_promise() { assert_eq!(default_weight("pro"),10.0); assert_eq!(default_weight("business"),1.0); }
    #[test] fn widget_model_scope_excludes_other_models_and_unavailable_accounts() {
        let mut a=sub("a","",None);a.subscription=false;a.available_models=vec!["claude-exact".into()];
        let mut b=a.clone();b.id="b".into();b.available_models=vec!["claude-other".into()];
        let mut c=a.clone();c.id="c".into();c.unavailable=true;
        let members=vec![a,b,c,sub("plus","plus",Some(true))];
        assert_eq!(members_for_model(&members,"claude-exact").len(),1);
        assert!(members_for_model(&members,"gpt-oss-120b").is_empty());
        assert!(members_for_model(&members,"").is_empty());
    }
    #[test] fn balance_capacity_requires_all_group_denominators_and_keeps_zero() {
        let mut a=sub("a","",None);a.subscription=false;
        a.balance=Some(Balance{amount:0.0,unit:"USD".into(),kind:"budget".into(),updated_at:1000,capacity:Some(50.0),estimated:false});
        let t=aggregate(&[a.clone()],1000);assert_eq!(t.balance_capacities["budget / USD"],50.0);
        let mut b=a.clone();b.identity="b".into();b.balance.as_mut().unwrap().capacity=None;
        assert!(aggregate(&[a,b],1000).balance_capacities.is_empty());
    }
    #[test] fn all_confirmed_unlimited_hides_short() { assert!(!aggregate(&[sub("p","business",Some(false))],1000).show_short); }
    #[test] fn monthly_only_quota_does_not_inflate_weekly_pool_capacity() {
        let mut monthly=sub("free","free",Some(false));monthly.weekly_supported=Some(false);monthly.weekly_left=None;
        let t=aggregate(&[sub("seat","team",Some(true)),monthly],1000);
        assert_eq!(t.weekly.capacity,100.0);assert_eq!(t.weekly.remaining,50.0);assert_eq!(t.weekly.unknown,0);
    }
    #[test] fn unknown_keeps_short_visible_and_stale_retains_last_known_with_marker() {
        let t=aggregate(&[sub("p","pro",None)],1000);assert!(t.show_short && t.short_uncertain);
        let t=aggregate(&[sub("p","pro",Some(true))],3000);
        assert_eq!(t.short.remaining,500.0);assert_eq!(t.short.stale,1);assert_eq!(t.weekly.stale,1);
        assert!(!aggregate(&[sub("p","pro",Some(false))],3000).show_short);
    }
    #[test] fn temporarily_unavailable_does_not_remove_window() { let mut m=sub("a","plus",Some(true)); m.unavailable=true; let t=aggregate(&[m],1000); assert!(t.show_short); assert_eq!(t.unavailable,1); }
    #[test] fn excluded_member_does_not_affect_pool() { let mut m=sub("a","plus",Some(true)); m.policy.included=false; let t=aggregate(&[m],1000); assert_eq!(t.members,0); assert!(!t.show_short); }
    #[test] fn duplicate_subscription_counted_once() { let m=sub("a","plus",Some(true)); let t=aggregate(&[m.clone(),m],1000); assert_eq!(t.weekly.remaining,50.0); assert_eq!(t.duplicates,1); }
    #[test] fn business_seats_sharing_workspace_stack_but_same_seat_reimport_deduplicates() {
        let mut a=sub("a","team",Some(true));let mut b=sub("b","team",Some(true));
        a.identity=subscription_identity(Some("workspace"),Some("user-a"),"a");
        b.identity=subscription_identity(Some("workspace"),Some("user-b"),"b");
        a.short_left=Some(100.0);a.weekly_left=Some(100.0);b.short_left=Some(100.0);b.weekly_left=Some(100.0);
        let t=aggregate(&[a.clone(),b.clone(),a],1000);
        assert_eq!(t.subscription_members,2);assert_eq!(t.duplicates,1);
        assert_eq!(t.short.remaining,200.0);assert_eq!(t.weekly.remaining,200.0);assert_eq!(t.short.capacity,200.0);
        assert_ne!(subscription_identity(Some("workspace"),None,"one"),subscription_identity(Some("workspace"),None,"two"));
    }
    #[test] fn shared_balance_and_currencies_remain_separate() {
        let make=|id: &str, group: &str, unit: &str, amount| { let mut m=sub(id,"",None); m.subscription=false; m.policy.billing_group=group.into(); m.balance=Some(Balance{amount,unit:unit.into(),kind:"balance".into(),updated_at:1000,capacity:None,estimated:false}); m.available_models=vec!["same".into()]; m };
        let t=aggregate(&[make("a","shared","USD",10.0),make("b","shared","USD",10.0),make("c","other","CNY",20.0)],1000);
        assert_eq!(t.balances["balance / USD"],10.0); assert_eq!(t.balances["balance / CNY"],20.0); assert_eq!(t.api_models,1); assert_eq!(t.api_sources,3); assert!(!t.show_short);
    }
    #[test] fn missing_and_stale_are_unknown_not_zero() { let mut m=sub("a","",None); m.subscription=false; let t=aggregate(&[m],1000); assert_eq!(t.balance_unknown,1); assert!(t.balances.is_empty()); }
    #[test] fn invalid_weights_and_intervals_rejected() { let mut p=SourcePolicy::default(); p.weekly_weight=Some(-1.0); assert!(p.validate().is_err()); p.weekly_weight=None; p.balance_minutes=0; assert!(p.validate().is_err()); }
    #[test] fn same_static_key_identity_deduplicates_without_manual_group() {
        let mut a=sub("a","",None);a.subscription=false;a.identity="same-key-and-host".into();
        a.balance=Some(Balance{amount:12.0,unit:"USD".into(),kind:"balance".into(),updated_at:1000,capacity:None,estimated:false});
        let mut b=a.clone();b.id="b".into();
        let t=aggregate(&[a,b],1000);assert_eq!(t.balances["balance / USD"],12.0);
    }

}
