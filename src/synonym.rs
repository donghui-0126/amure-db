/// Synonym — 한/영 퀀트 용어 동의어 사전.
/// 토큰을 넣으면 동의어 그룹 전체를 반환.

use std::collections::HashMap;

pub struct SynonymDict {
    /// token → group_id
    token_to_group: HashMap<String, usize>,
    /// group_id → all tokens in that group
    groups: Vec<Vec<String>>,
}

impl SynonymDict {
    pub fn new() -> Self {
        let mut dict = Self {
            token_to_group: HashMap::new(),
            groups: Vec::new(),
        };
        dict.load_defaults();
        dict
    }

    /// 동의어 그룹 추가
    pub fn add_group(&mut self, synonyms: &[&str]) {
        let group_id = self.groups.len();
        let tokens: Vec<String> = synonyms.iter().map(|s| s.to_lowercase()).collect();
        for t in &tokens {
            self.token_to_group.insert(t.clone(), group_id);
        }
        self.groups.push(tokens);
    }

    /// 토큰의 동의어 목록 반환 (자기 자신 포함)
    pub fn expand(&self, token: &str) -> Vec<String> {
        let lower = token.to_lowercase();
        if let Some(&group_id) = self.token_to_group.get(&lower) {
            self.groups[group_id].clone()
        } else {
            vec![lower]
        }
    }

    /// 여러 토큰을 동의어 확장
    pub fn expand_all(&self, tokens: &[String]) -> Vec<String> {
        let mut expanded = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for t in tokens {
            for syn in self.expand(t) {
                if seen.insert(syn.clone()) {
                    expanded.push(syn);
                }
            }
        }
        expanded
    }

    fn load_defaults(&mut self) {
        // 시장 데이터
        self.add_group(&["oi", "open_interest", "미결제약정"]);
        self.add_group(&["volume", "vol", "거래량"]);
        self.add_group(&["funding", "funding_rate", "펀딩", "펀딩레이트"]);
        self.add_group(&["premium", "프리미엄", "베이시스", "basis"]);
        self.add_group(&["close", "종가", "price", "가격"]);

        // 분석 개념
        self.add_group(&["momentum", "모멘텀", "추세"]);
        self.add_group(&["mean_reversion", "평균회귀", "회귀"]);
        self.add_group(&["volatility", "변동성", "vol_regime"]);
        self.add_group(&["cross_sectional", "횡단면", "cs"]);
        self.add_group(&["ic", "information_coefficient", "정보계수"]);
        self.add_group(&["regime", "레짐", "시장국면"]);
        self.add_group(&["backtest", "백테스트"]);
        self.add_group(&["alpha", "알파", "초과수익"]);
        self.add_group(&["beta", "베타"]);
        self.add_group(&["sharpe", "샤프"]);
        self.add_group(&["drawdown", "mdd", "낙폭"]);
        self.add_group(&["correlation", "상관관계", "corr"]);
        self.add_group(&["decay", "감쇠", "디케이"]);
        self.add_group(&["continuation", "지속", "연속"]);
        self.add_group(&["reversal", "반전", "역전"]);
        self.add_group(&["cascade", "캐스케이드", "연쇄"]);
        self.add_group(&["liquidation", "청산"]);
        self.add_group(&["conviction", "확신", "신념"]);

        // 유니버스/레짐
        self.add_group(&["bull", "상승장", "강세"]);
        self.add_group(&["bear", "하락장", "약세"]);
        self.add_group(&["crash", "급락", "폭락"]);
        self.add_group(&["sideways", "횡보", "보합"]);
        self.add_group(&["large_cap", "대형주", "mcap_large"]);
        self.add_group(&["small_cap", "소형주", "mcap_small"]);

        // 시장 구조
        self.add_group(&["futures", "선물", "fut"]);
        self.add_group(&["spot", "현물"]);
        self.add_group(&["crypto", "크립토", "암호화폐"]);
        self.add_group(&["binance", "바이낸스"]);
    }
}

impl Default for SynonymDict {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_basic() {
        let dict = SynonymDict::new();
        let syns = dict.expand("OI");
        assert!(syns.contains(&"oi".to_string()));
        assert!(syns.contains(&"open_interest".to_string()));
        assert!(syns.contains(&"미결제약정".to_string()));
    }

    #[test]
    fn test_expand_korean() {
        let dict = SynonymDict::new();
        let syns = dict.expand("모멘텀");
        assert!(syns.contains(&"momentum".to_string()));
    }

    #[test]
    fn test_expand_unknown() {
        let dict = SynonymDict::new();
        let syns = dict.expand("xyzabc");
        assert_eq!(syns, vec!["xyzabc".to_string()]);
    }

    #[test]
    fn test_expand_all() {
        let dict = SynonymDict::new();
        let tokens = vec!["OI".to_string(), "momentum".to_string()];
        let expanded = dict.expand_all(&tokens);
        assert!(expanded.contains(&"open_interest".to_string()));
        assert!(expanded.contains(&"모멘텀".to_string()));
        // No duplicates
        let unique: std::collections::HashSet<_> = expanded.iter().collect();
        assert_eq!(unique.len(), expanded.len());
    }
}
