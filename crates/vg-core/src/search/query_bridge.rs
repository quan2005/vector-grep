use std::borrow::Cow;

struct QueryAliasRule {
    trigger: &'static str,
    aliases: &'static [&'static str],
}

const QUERY_ALIAS_RULES: &[QueryAliasRule] = &[
    QueryAliasRule {
        trigger: "认证授权",
        aliases: &[
            "auth",
            "authn",
            "authz",
            "authentication",
            "authorization",
            "oauth",
            "oauth2",
            "token",
            "login",
            "permission",
            "rbac",
            "abac",
        ],
    },
    QueryAliasRule {
        trigger: "认证",
        aliases: &[
            "auth",
            "authn",
            "authentication",
            "oauth",
            "oauth2",
            "token",
            "login",
        ],
    },
    QueryAliasRule {
        trigger: "授权",
        aliases: &["authz", "authorization", "permission", "rbac", "abac"],
    },
    QueryAliasRule {
        trigger: "错误处理",
        aliases: &[
            "error",
            "errors",
            "exception",
            "exceptions",
            "fail",
            "failure",
            "retry",
            "timeout",
            "fallback",
        ],
    },
    QueryAliasRule {
        trigger: "异常处理",
        aliases: &[
            "error",
            "exception",
            "exceptions",
            "retry",
            "timeout",
            "fallback",
        ],
    },
    QueryAliasRule {
        trigger: "性能优化",
        aliases: &[
            "performance",
            "latency",
            "throughput",
            "bottleneck",
            "cache",
            "caching",
            "profile",
            "optimize",
            "optimization",
        ],
    },
    QueryAliasRule {
        trigger: "性能",
        aliases: &["performance", "latency", "throughput", "bottleneck"],
    },
];

pub(crate) fn build_query_variants(query: &str) -> Vec<Cow<'_, str>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return vec![Cow::Borrowed(trimmed)];
    }

    let compact = compact_for_match(trimmed);
    let mut bridge_terms = Vec::new();
    for rule in QUERY_ALIAS_RULES {
        if !compact.contains(rule.trigger) {
            continue;
        }

        for alias in rule.aliases {
            if contains_ascii_alias(trimmed, alias) || bridge_terms.contains(alias) {
                continue;
            }
            bridge_terms.push(*alias);
        }
    }

    if bridge_terms.is_empty() {
        vec![Cow::Borrowed(trimmed)]
    } else {
        vec![Cow::Borrowed(trimmed), Cow::Owned(bridge_terms.join(" "))]
    }
}

fn compact_for_match(query: &str) -> String {
    query
        .chars()
        .filter(|ch| !ch.is_whitespace() && !is_separator(*ch))
        .collect()
}

fn contains_ascii_alias(query: &str, alias: &str) -> bool {
    query
        .to_ascii_lowercase()
        .contains(&alias.to_ascii_lowercase())
}

fn is_separator(ch: char) -> bool {
    matches!(
        ch,
        '/' | '\\'
            | '|'
            | '+'
            | '-'
            | '_'
            | ','
            | '.'
            | '?'
            | '!'
            | ':'
            | ';'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '，'
            | '。'
            | '？'
            | '！'
            | '：'
            | '；'
            | '（'
            | '）'
            | '【'
            | '】'
            | '、'
    )
}

#[cfg(test)]
mod tests {
    use super::build_query_variants;

    #[test]
    fn build_query_variants_adds_bridge_query_for_cjk_terms() {
        let variants = build_query_variants("认证授权");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].as_ref(), "认证授权");
        assert_eq!(
            variants[1].as_ref(),
            "auth authn authz authentication authorization oauth oauth2 token login permission rbac abac"
        );
    }

    #[test]
    fn build_query_variants_handles_punctuation_between_cjk_terms() {
        let variants = build_query_variants("错误 / 处理");
        assert_eq!(variants[0].as_ref(), "错误 / 处理");
        assert!(variants[1].as_ref().contains("error"));
        assert!(variants[1].as_ref().contains("exception"));
        assert!(variants[1].as_ref().contains("timeout"));
    }

    #[test]
    fn build_query_variants_does_not_duplicate_existing_aliases() {
        let variants = build_query_variants("错误处理 error timeout");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].as_ref(), "错误处理 error timeout");
        assert_eq!(
            variants[1].as_ref(),
            "errors exception exceptions fail failure retry fallback"
        );
    }

    #[test]
    fn build_query_variants_keeps_unknown_queries_unchanged() {
        let variants = build_query_variants("知识管理");
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].as_ref(), "知识管理");
    }
}
