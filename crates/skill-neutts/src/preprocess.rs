//! Text preprocessing pipeline — expands numbers, symbols, abbreviations, etc.
//! before phonemisation.

use fancy_regex::{Captures, Regex};
use once_cell::sync::Lazy;
use std::borrow::Cow;

const ONES: &[&str] = &[
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: &[&str] = &[
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALE: &[&str] = &["", "thousand", "million", "billion", "trillion"];

fn three_digits_to_words(n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    let mut parts = Vec::new();
    let hundreds = n / 100;
    let remainder = n % 100;
    if hundreds > 0 {
        parts.push(format!("{} hundred", ONES[hundreds as usize]));
    }
    if remainder < 20 {
        if remainder > 0 {
            parts.push(ONES[remainder as usize].to_string());
        }
    } else {
        let tens_word = TENS[(remainder / 10) as usize];
        let ones_word = ONES[(remainder % 10) as usize];
        if ones_word.is_empty() {
            parts.push(tens_word.to_string());
        } else {
            parts.push(format!("{}-{}", tens_word, ones_word));
        }
    }
    parts.join(" ")
}

pub fn number_to_words(n: i64) -> String {
    if n < 0 {
        return format!("negative {}", number_to_words(-n));
    }
    let n = n as u64;
    if n == 0 {
        return "zero".to_string();
    }
    if n >= 100 && n <= 9999 && n % 100 == 0 && n % 1000 != 0 {
        let hundreds = n / 100;
        if hundreds < 20 {
            return format!("{} hundred", ONES[hundreds as usize]);
        }
    }
    let mut parts = Vec::new();
    let mut remaining = n;
    for &scale in SCALE.iter() {
        let chunk = remaining % 1000;
        if chunk > 0 {
            let chunk_words = three_digits_to_words(chunk);
            if scale.is_empty() {
                parts.push(chunk_words);
            } else {
                parts.push(format!("{} {}", chunk_words, scale));
            }
        }
        remaining /= 1000;
        if remaining == 0 {
            break;
        }
    }
    parts.reverse();
    parts.join(" ")
}

pub fn float_to_words(value: &str) -> String {
    let negative = value.starts_with('-');
    let value = if negative { &value[1..] } else { value };

    let digit_words = |c: char| match c {
        '0' => "zero",
        '1' => "one",
        '2' => "two",
        '3' => "three",
        '4' => "four",
        '5' => "five",
        '6' => "six",
        '7' => "seven",
        '8' => "eight",
        '9' => "nine",
        _ => "",
    };

    let result = if let Some(dot) = value.find('.') {
        let int_part = &value[..dot];
        let dec_part = &value[dot + 1..];
        let int_words = if int_part.is_empty() {
            "zero".to_string()
        } else {
            number_to_words(int_part.parse::<i64>().unwrap_or(0))
        };
        let dec_words: Vec<&str> = dec_part.chars().map(digit_words).collect();
        format!("{} point {}", int_words, dec_words.join(" "))
    } else {
        number_to_words(value.parse::<i64>().unwrap_or(0))
    };

    if negative {
        format!("negative {}", result)
    } else {
        result
    }
}

fn ordinal_suffix(n: i64) -> String {
    let word = number_to_words(n);
    let exceptions = [
        ("one", "first"),
        ("two", "second"),
        ("three", "third"),
        ("four", "fourth"),
        ("five", "fifth"),
        ("six", "sixth"),
        ("seven", "seventh"),
        ("eight", "eighth"),
        ("nine", "ninth"),
        ("twelve", "twelfth"),
    ];
    let (prefix, last, sep) = if let Some(pos) = word.rfind('-') {
        (&word[..pos], &word[pos + 1..], "-")
    } else if let Some(pos) = word.rfind(' ') {
        (&word[..pos], &word[pos + 1..], " ")
    } else {
        ("", word.as_str(), "")
    };

    let last_ord = exceptions
        .iter()
        .find(|(base, _)| *base == last)
        .map(|(_, ord)| (*ord).to_string())
        .unwrap_or_else(|| {
            if last.ends_with('t') {
                format!("{}h", last)
            } else if let Some(stripped) = last.strip_suffix('e') {
                format!("{}th", stripped)
            } else if let Some(stripped) = last.strip_suffix('y') {
                format!("{}ieth", stripped)
            } else {
                format!("{}th", last)
            }
        });

    if prefix.is_empty() {
        last_ord
    } else {
        format!("{}{}{}", prefix, sep, last_ord)
    }
}

static RE_ORDINAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\d+)(st|nd|rd|th)\b").unwrap());
static RE_PERCENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(-?[\d,]+(?:\.\d+)?)\s*%").unwrap());
static RE_CURRENCY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([$€£¥₹₩₿])\s*([\d,]+(?:\.\d+)?)\s*([KMBT])?(?![a-zA-Z\d])").unwrap());
static RE_TIME: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\d{1,2}):(\d{2})(?::\d{2})?\s*(am|pm)?\b").unwrap());
static RE_RANGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\w)(\d+)-(\d+)(?!\w)").unwrap());
static RE_MODEL_VER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b([a-zA-Z][a-zA-Z0-9]*)-(\d[\d.]*)(?=[^\d.]|$)").unwrap());
static RE_UNIT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*(km|kg|mg|ml|gb|mb|kb|tb|hz|khz|mhz|ghz|mph|kph|°[cCfF]|[cCfF]°|ms|ns|µs)\b")
        .unwrap()
});
static RE_SCALE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?<![a-zA-Z])(\d+(?:\.\d+)?)\s*([KMBT])(?![a-zA-Z\d])").unwrap());
static RE_SCI: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?<![a-zA-Z\d])(-?\d+(?:\.\d+)?)[eE]([+-]?\d+)(?![a-zA-Z\d])").unwrap());
static RE_FRACTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d+)\s*/\s*(\d+)\b").unwrap());
static RE_DECADE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,3})0s\b").unwrap());
static RE_LEAD_DEC: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\d)\.(\d)").unwrap());
static RE_NEG_LEAD_DEC: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\d)(-)\.(\d)").unwrap());
static RE_NUMBER: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<![a-zA-Z])-?[\d,]+(?:\.\d+)?").unwrap());
static RE_IP: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").unwrap());
static RE_PHONE_11: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?<!\d-)\b(\d{1,2})-(\d{3})-(\d{3})-(\d{4})\b(?!-\d)").unwrap());
static RE_PHONE_10: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\d-)\b(\d{3})-(\d{3})-(\d{4})\b(?!-\d)").unwrap());
static RE_PHONE_7: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\d-)\b(\d{3})-(\d{4})\b(?!-\d)").unwrap());
static RE_URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://\S+|www\.\S+").unwrap());
static RE_EMAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b[\w.+-]+@[\w-]+\.[a-z]{2,}\b").unwrap());
static RE_HTML: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static RE_SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static RE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\w\s]").unwrap());
static RE_CONTRACTION_CANT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bcan't\b").unwrap());
static RE_CONTRACTION_WONT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bwon't\b").unwrap());
static RE_CONTRACTION_SHANT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bshan't\b").unwrap());
static RE_CONTRACTION_AINT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bain't\b").unwrap());
static RE_CONTRACTION_LETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\blet's\b").unwrap());
static RE_CONTRACTION_ITS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bit's\b").unwrap());
static RE_CONTRACTION_NT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)n't\b").unwrap());
static RE_CONTRACTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)'re\b").unwrap());
static RE_CONTRACTION_VE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)'ve\b").unwrap());
static RE_CONTRACTION_LL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)'ll\b").unwrap());
static RE_CONTRACTION_D: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)'d\b").unwrap());
static RE_CONTRACTION_M: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(\w+)'m\b").unwrap());

fn currency_symbol_name(sym: &str) -> &'static str {
    match sym {
        "$" => "dollar",
        "€" => "euro",
        "£" => "pound",
        "¥" => "yen",
        "₹" => "rupee",
        "₩" => "won",
        "₿" => "bitcoin",
        _ => "",
    }
}

fn scale_suffix_word(s: &str) -> &'static str {
    match s {
        "K" => "thousand",
        "M" => "million",
        "B" => "billion",
        "T" => "trillion",
        _ => "",
    }
}

fn digits_to_words(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => "zero",
            '1' => "one",
            '2' => "two",
            '3' => "three",
            '4' => "four",
            '5' => "five",
            '6' => "six",
            '7' => "seven",
            '8' => "eight",
            '9' => "nine",
            _ => "",
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn unit_expansion(unit: &str) -> &'static str {
    match unit.to_lowercase().as_str() {
        "km" => "kilometers",
        "kg" => "kilograms",
        "mg" => "milligrams",
        "ml" => "milliliters",
        "gb" => "gigabytes",
        "mb" => "megabytes",
        "kb" => "kilobytes",
        "tb" => "terabytes",
        "hz" => "hertz",
        "khz" => "kilohertz",
        "mhz" => "megahertz",
        "ghz" => "gigahertz",
        "mph" => "miles per hour",
        "kph" => "kilometers per hour",
        "ms" => "milliseconds",
        "ns" => "nanoseconds",
        "µs" => "microseconds",
        "°c" | "c°" => "degrees Celsius",
        "°f" | "f°" => "degrees Fahrenheit",
        _ => "",
    }
}

pub fn expand_ordinals(text: &str) -> String {
    RE_ORDINAL
        .replace_all(text, |caps: &Captures| {
            let n: i64 = caps[1].parse().unwrap_or(0);
            ordinal_suffix(n)
        })
        .into_owned()
}

pub fn expand_percentages(text: &str) -> String {
    RE_PERCENT
        .replace_all(text, |caps: &Captures| {
            let raw = caps[1].replace(',', "");
            let words = if raw.contains('.') {
                float_to_words(&raw)
            } else {
                number_to_words(raw.parse::<i64>().unwrap_or(0))
            };
            format!("{} percent", words)
        })
        .into_owned()
}

pub fn expand_currency(text: &str) -> String {
    RE_CURRENCY
        .replace_all(text, |caps: &Captures| {
            let symbol = &caps[1];
            let raw = caps[2].replace(',', "");
            let scale_suffix = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let unit = currency_symbol_name(symbol);

            if !scale_suffix.is_empty() {
                let scale_word = scale_suffix_word(scale_suffix);
                let num = if raw.contains('.') {
                    float_to_words(&raw)
                } else {
                    number_to_words(raw.parse::<i64>().unwrap_or(0))
                };
                return format!("{} {} {}s", num, scale_word, unit);
            }

            if raw.contains('.') {
                let dot = raw.find('.').unwrap();
                let int_part: i64 = raw[..dot].parse().unwrap_or(0);
                let dec_str = &raw[dot + 1..];
                let dec_val: i64 = dec_str[..dec_str.len().min(2)]
                    .chars()
                    .chain(std::iter::repeat('0'))
                    .take(2)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                let int_words = number_to_words(int_part);
                let mut result = if unit.is_empty() {
                    int_words
                } else {
                    format!("{} {}s", int_words, unit)
                };
                if dec_val > 0 {
                    let cents = number_to_words(dec_val);
                    let plural = if dec_val == 1 { "" } else { "s" };
                    result.push_str(&format!(" and {} cent{}", cents, plural));
                }
                result
            } else {
                let val: i64 = raw.parse().unwrap_or(0);
                let words = number_to_words(val);
                if unit.is_empty() {
                    words
                } else {
                    let plural = if val == 1 { "" } else { "s" };
                    format!("{} {}{}", words, unit, plural)
                }
            }
        })
        .into_owned()
}

pub fn expand_time(text: &str) -> String {
    RE_TIME
        .replace_all(text, |caps: &Captures| {
            let h: i64 = caps[1].parse().unwrap_or(0);
            let mins: i64 = caps[2].parse().unwrap_or(0);
            let suffix = caps
                .get(3)
                .map(|m| format!(" {}", m.as_str().to_lowercase()))
                .unwrap_or_default();
            let h_words = number_to_words(h);
            if mins == 0 {
                if caps.get(3).is_some() {
                    format!("{}{}", h_words, suffix)
                } else {
                    format!("{} hundred", h_words)
                }
            } else if mins < 10 {
                format!("{} oh {}{}", h_words, number_to_words(mins), suffix)
            } else {
                format!("{} {}{}", h_words, number_to_words(mins), suffix)
            }
        })
        .into_owned()
}

pub fn expand_ranges(text: &str) -> String {
    RE_RANGE
        .replace_all(text, |caps: &Captures| {
            let lo = number_to_words(caps[1].parse().unwrap_or(0));
            let hi = number_to_words(caps[2].parse().unwrap_or(0));
            format!("{} to {}", lo, hi)
        })
        .into_owned()
}

pub fn expand_model_names(text: &str) -> String {
    RE_MODEL_VER
        .replace_all(text, |caps: &Captures| format!("{} {}", &caps[1], &caps[2]))
        .into_owned()
}

pub fn expand_units(text: &str) -> String {
    RE_UNIT
        .replace_all(text, |caps: &Captures| {
            let raw = &caps[1];
            let unit = &caps[2];
            let expanded = unit_expansion(unit);
            let expanded = if expanded.is_empty() { unit } else { expanded };
            let num = if raw.contains('.') {
                float_to_words(raw)
            } else {
                number_to_words(raw.parse::<i64>().unwrap_or(0))
            };
            format!("{} {}", num, expanded)
        })
        .into_owned()
}

pub fn expand_scale_suffixes(text: &str) -> String {
    RE_SCALE
        .replace_all(text, |caps: &Captures| {
            let raw = &caps[1];
            let suffix = &caps[2];
            let scale_word = scale_suffix_word(suffix);
            let num = if raw.contains('.') {
                float_to_words(raw)
            } else {
                number_to_words(raw.parse::<i64>().unwrap_or(0))
            };
            format!("{} {}", num, scale_word)
        })
        .into_owned()
}

pub fn expand_scientific_notation(text: &str) -> String {
    RE_SCI
        .replace_all(text, |caps: &Captures| {
            let coeff = &caps[1];
            let exp: i64 = caps[2].parse().unwrap_or(0);
            let coeff_words = if coeff.contains('.') {
                float_to_words(coeff)
            } else {
                number_to_words(coeff.parse::<i64>().unwrap_or(0))
            };
            let exp_words = number_to_words(exp.abs());
            let sign = if exp < 0 { "negative " } else { "" };
            format!("{} times ten to the {}{}", coeff_words, sign, exp_words)
        })
        .into_owned()
}

pub fn expand_fractions(text: &str) -> String {
    RE_FRACTION
        .replace_all(text, |caps: &Captures| {
            let num: i64 = caps[1].parse().unwrap_or(0);
            let den: i64 = caps[2].parse().unwrap_or(1);
            if den == 0 {
                return caps[0].to_string();
            }
            let num_words = number_to_words(num);
            let denom_word = match den {
                2 => (if num == 1 { "half" } else { "halves" }).to_string(),
                4 => (if num == 1 { "quarter" } else { "quarters" }).to_string(),
                _ => {
                    let ord = ordinal_suffix(den);
                    if num != 1 {
                        format!("{}s", ord)
                    } else {
                        ord
                    }
                }
            };
            format!("{} {}", num_words, denom_word)
        })
        .into_owned()
}

pub fn expand_decades(text: &str) -> String {
    let decade_map = [
        (0, "hundreds"),
        (1, "tens"),
        (2, "twenties"),
        (3, "thirties"),
        (4, "forties"),
        (5, "fifties"),
        (6, "sixties"),
        (7, "seventies"),
        (8, "eighties"),
        (9, "nineties"),
    ];
    RE_DECADE
        .replace_all(text, |caps: &Captures| {
            let base: i64 = caps[1].parse().unwrap_or(0);
            let decade_digit = (base % 10) as usize;
            let decade_word = decade_map
                .iter()
                .find(|(d, _)| *d == decade_digit as i64)
                .map(|(_, w)| *w)
                .unwrap_or("");
            if base < 10 {
                decade_word.to_string()
            } else {
                let century_part = base / 10;
                format!("{} {}", number_to_words(century_part), decade_word)
            }
        })
        .into_owned()
}

pub fn expand_ip_addresses(text: &str) -> String {
    RE_IP
        .replace_all(text, |caps: &Captures| {
            let octets: Vec<String> = (1..=4).map(|i| digits_to_words(&caps[i])).collect();
            octets.join(" dot ")
        })
        .into_owned()
}

pub fn expand_phone_numbers(text: &str) -> String {
    let join_groups =
        |groups: Vec<&str>| -> String { groups.iter().map(|g| digits_to_words(g)).collect::<Vec<_>>().join(" ") };
    let text = RE_PHONE_11
        .replace_all(text, |caps: &Captures| {
            join_groups(vec![&caps[1], &caps[2], &caps[3], &caps[4]])
        })
        .into_owned();
    let text = RE_PHONE_10
        .replace_all(&text, |caps: &Captures| join_groups(vec![&caps[1], &caps[2], &caps[3]]))
        .into_owned();
    RE_PHONE_7
        .replace_all(&text, |caps: &Captures| join_groups(vec![&caps[1], &caps[2]]))
        .into_owned()
}

pub fn normalize_leading_decimals(text: &str) -> String {
    let text = RE_NEG_LEAD_DEC
        .replace_all(text, |caps: &Captures| format!("{}0.{}", &caps[1], &caps[2]))
        .into_owned();
    RE_LEAD_DEC
        .replace_all(&text, |caps: &Captures| format!("0.{}", &caps[1]))
        .into_owned()
}

pub fn replace_numbers(text: &str) -> String {
    RE_NUMBER
        .replace_all(text, |caps: &Captures| {
            let raw = caps[0].replace(',', "");
            if raw.contains('.') {
                float_to_words(&raw)
            } else if let Ok(n) = raw.parse::<i64>() {
                number_to_words(n)
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}

pub fn expand_contractions(text: &str) -> String {
    let text = RE_CONTRACTION_CANT.replace_all(text, "cannot").into_owned();
    let text = RE_CONTRACTION_WONT.replace_all(&text, "will not").into_owned();
    let text = RE_CONTRACTION_SHANT.replace_all(&text, "shall not").into_owned();
    let text = RE_CONTRACTION_AINT.replace_all(&text, "is not").into_owned();
    let text = RE_CONTRACTION_LETS.replace_all(&text, "let us").into_owned();
    let text = RE_CONTRACTION_ITS.replace_all(&text, "it is").into_owned();
    let text = RE_CONTRACTION_NT.replace_all(&text, "$1 not").into_owned();
    let text = RE_CONTRACTION_RE.replace_all(&text, "$1 are").into_owned();
    let text = RE_CONTRACTION_VE.replace_all(&text, "$1 have").into_owned();
    let text = RE_CONTRACTION_LL.replace_all(&text, "$1 will").into_owned();
    let text = RE_CONTRACTION_D.replace_all(&text, "$1 would").into_owned();
    RE_CONTRACTION_M.replace_all(&text, "$1 am").into_owned()
}

pub fn remove_urls(text: &str) -> Cow<'_, str> {
    RE_URL.replace_all(text, "")
}

pub fn remove_emails(text: &str) -> Cow<'_, str> {
    RE_EMAIL.replace_all(text, "")
}

pub fn remove_html_tags(text: &str) -> Cow<'_, str> {
    RE_HTML.replace_all(text, " ")
}

pub fn remove_punctuation(text: &str) -> Cow<'_, str> {
    RE_PUNCT.replace_all(text, " ")
}

pub fn remove_extra_whitespace(text: &str) -> String {
    RE_SPACES.replace_all(text.trim(), " ").into_owned()
}

#[derive(Debug, Clone)]
pub struct PreprocessorConfig {
    pub lowercase: bool,
    pub replace_numbers: bool,
    pub expand_contractions: bool,
    pub expand_model_names: bool,
    pub expand_ordinals: bool,
    pub expand_percentages: bool,
    pub expand_currency: bool,
    pub expand_time: bool,
    pub expand_ranges: bool,
    pub expand_units: bool,
    pub expand_scale_suffixes: bool,
    pub expand_scientific_notation: bool,
    pub expand_fractions: bool,
    pub expand_decades: bool,
    pub expand_phone_numbers: bool,
    pub expand_ip_addresses: bool,
    pub normalize_leading_decimals: bool,
    pub remove_urls: bool,
    pub remove_emails: bool,
    pub remove_html: bool,
    pub remove_punctuation: bool,
    pub remove_extra_whitespace: bool,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self {
            lowercase: true,
            replace_numbers: true,
            expand_contractions: true,
            expand_model_names: true,
            expand_ordinals: true,
            expand_percentages: true,
            expand_currency: true,
            expand_time: true,
            expand_ranges: true,
            expand_units: true,
            expand_scale_suffixes: true,
            expand_scientific_notation: true,
            expand_fractions: true,
            expand_decades: true,
            expand_phone_numbers: true,
            expand_ip_addresses: true,
            normalize_leading_decimals: true,
            remove_urls: true,
            remove_emails: true,
            remove_html: true,
            remove_punctuation: true,
            remove_extra_whitespace: true,
        }
    }
}

#[derive(Default)]
pub struct TextPreprocessor {
    pub config: PreprocessorConfig,
}

impl TextPreprocessor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: PreprocessorConfig) -> Self {
        Self { config }
    }

    pub fn process(&self, text: &str) -> String {
        let cfg = &self.config;
        let mut text = text.to_string();

        if cfg.remove_html {
            text = remove_html_tags(&text).into_owned();
        }
        if cfg.remove_urls {
            text = remove_urls(&text).into_owned();
        }
        if cfg.remove_emails {
            text = remove_emails(&text).into_owned();
        }
        if cfg.expand_contractions {
            text = expand_contractions(&text);
        }
        if cfg.expand_ip_addresses {
            text = expand_ip_addresses(&text);
        }
        if cfg.normalize_leading_decimals {
            text = normalize_leading_decimals(&text);
        }
        if cfg.expand_currency {
            text = expand_currency(&text);
        }
        if cfg.expand_percentages {
            text = expand_percentages(&text);
        }
        if cfg.expand_scientific_notation {
            text = expand_scientific_notation(&text);
        }
        if cfg.expand_time {
            text = expand_time(&text);
        }
        if cfg.expand_ordinals {
            text = expand_ordinals(&text);
        }
        if cfg.expand_units {
            text = expand_units(&text);
        }
        if cfg.expand_scale_suffixes {
            text = expand_scale_suffixes(&text);
        }
        if cfg.expand_fractions {
            text = expand_fractions(&text);
        }
        if cfg.expand_decades {
            text = expand_decades(&text);
        }
        if cfg.expand_phone_numbers {
            text = expand_phone_numbers(&text);
        }
        if cfg.expand_ranges {
            text = expand_ranges(&text);
        }
        if cfg.expand_model_names {
            text = expand_model_names(&text);
        }
        if cfg.replace_numbers {
            text = replace_numbers(&text);
        }
        if cfg.remove_punctuation {
            text = remove_punctuation(&text).into_owned();
        }
        if cfg.lowercase {
            text = text.to_lowercase();
        }
        if cfg.remove_extra_whitespace {
            text = remove_extra_whitespace(&text);
        }

        text
    }
}
