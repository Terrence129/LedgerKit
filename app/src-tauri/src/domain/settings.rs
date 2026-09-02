#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiLocale {
    EnUs,
    ZhCn,
}

impl UiLocale {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "en-US" => Some(Self::EnUs),
            "zh-CN" => Some(Self::ZhCn),
            _ => None,
        }
    }

    pub fn from_system_hint(value: Option<&str>) -> Self {
        let is_chinese = value.is_some_and(|locale| {
            locale
                .split(['-', '_'])
                .next()
                .is_some_and(|language| language.eq_ignore_ascii_case("zh"))
        });
        if is_chinese { Self::ZhCn } else { Self::EnUs }
    }
}

#[cfg(test)]
mod tests {
    use super::UiLocale;

    #[test]
    fn first_launch_uses_chinese_for_chinese_windows_hints() {
        for locale in ["zh-CN", "zh-Hans-CN", "ZH_sg"] {
            assert_eq!(UiLocale::from_system_hint(Some(locale)), UiLocale::ZhCn);
        }
    }

    #[test]
    fn first_launch_falls_back_to_english() {
        for locale in [None, Some("en-SG"), Some("fr-FR"), Some("")] {
            assert_eq!(UiLocale::from_system_hint(locale), UiLocale::EnUs);
        }
    }
}
