//! LaTeX 词法/语法解析:输出与 XML 无关的 `Elm` 语法树。

use super::symbols::{symbol, upright_fn};

#[derive(Debug, Clone)]
pub(super) enum Elm {
    Run(String, bool),
    Seq(Vec<Elm>),
    Frac(Vec<Elm>, Vec<Elm>),
    Rad(Option<Vec<Elm>>, Vec<Elm>),
    Script {
        base: Vec<Elm>,
        sub: Option<Vec<Elm>>,
        sup: Option<Vec<Elm>>,
    },
    Delim(Option<char>, Option<char>, Vec<Elm>),
    Accent(char, Vec<Elm>),
}

pub(super) struct Parser {
    s: Vec<char>,
    i: usize,
    stop: Option<char>,
}

impl Parser {
    pub(super) fn new(src: &str) -> Self {
        Parser {
            s: src.chars().collect(),
            i: 0,
            stop: None,
        }
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// `\right` 停止符检测(不消费,且不误匹配 `\rightarrow`)。
    fn at_right(&self) -> bool {
        self.s[self.i..].iter().take(6).copied().collect::<String>() == "\\right"
            && !self
                .s
                .get(self.i + 6)
                .is_some_and(|c| c.is_ascii_alphabetic())
    }

    fn read_cmd(&mut self) -> String {
        let mut cmd = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                cmd.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        cmd
    }

    fn read_group_raw(&mut self) -> String {
        self.skip_ws();
        if !self.eat('{') {
            return String::new();
        }
        let mut depth = 1usize;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                '{' => {
                    depth += 1;
                    out.push(c);
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        out
    }

    fn parse_arg(&mut self) -> Vec<Elm> {
        self.skip_ws();
        if self.eat('{') {
            let inner = self.parse_seq(true, false);
            self.eat('}');
            return inner;
        }
        self.parse_atom().into_iter().collect()
    }

    fn parse_until(&mut self, brace: bool, right: bool) -> Vec<Elm> {
        let mut out = Vec::new();
        while self.i < self.s.len() {
            self.skip_ws();
            let Some(c) = self.peek() else { break };
            if brace && c == '}' {
                break;
            }
            if right && self.at_right() {
                break;
            }
            if self.stop == Some(c) {
                break;
            }
            if c == '^' || c == '_' {
                self.attach_script(&mut out);
                continue;
            }
            if c == '{' {
                self.i += 1;
                let inner = self.parse_until(true, false);
                self.eat('}');
                out.push(Elm::Seq(inner));
                continue;
            }
            if c == '}' {
                break;
            }
            if c == '&' {
                self.i += 1;
                continue;
            }
            if let Some(e) = self.parse_atom() {
                out.push(e);
            }
        }
        out
    }

    // 兼容旧调用:最初的 parse_seq 与 parse_until 相同语义
    pub(super) fn parse_seq(&mut self, brace: bool, right: bool) -> Vec<Elm> {
        self.parse_until(brace, right)
    }

    fn attach_script(&mut self, out: &mut Vec<Elm>) {
        let (mut sub, mut sup) = (None, None);
        loop {
            self.skip_ws();
            match self.peek() {
                Some('^') => {
                    self.i += 1;
                    sup = Some(self.parse_arg());
                }
                Some('_') => {
                    self.i += 1;
                    sub = Some(self.parse_arg());
                }
                _ => break,
            }
        }
        let base = out.pop().map(|e| vec![e]).unwrap_or_default();
        out.push(Elm::Script { base, sub, sup });
    }

    fn parse_atom(&mut self) -> Option<Elm> {
        self.skip_ws();
        let c = self.peek()?;
        match c {
            '\\' => self.parse_command(),
            '{' => {
                self.i += 1;
                let inner = self.parse_until(true, false);
                self.eat('}');
                Some(Elm::Seq(inner))
            }
            '$' => {
                self.i += 1;
                None
            }
            _ => {
                let mut text = String::new();
                while let Some(ch) = self.peek() {
                    if matches!(ch, '\\' | '{' | '}' | '^' | '_' | '&' | '$' | '~')
                        || ch.is_whitespace()
                    {
                        break;
                    }
                    text.push(ch);
                    self.i += 1;
                }
                if text.is_empty() {
                    self.i += 1;
                    None
                } else {
                    Some(Elm::Run(text, false))
                }
            }
        }
    }

    fn parse_command(&mut self) -> Option<Elm> {
        self.i += 1; // '\'
        let cmd = self.read_cmd();
        if cmd.is_empty() {
            let ch = self.peek()?;
            self.i += 1;
            return match ch {
                '\\' => Some(Elm::Run(" ; ".into(), true)),
                ',' | ';' | ':' | '!' | ' ' => Some(Elm::Run(" ".into(), true)),
                _ => Some(Elm::Run(ch.to_string(), true)),
            };
        }
        match cmd.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.parse_arg();
                let b = self.parse_arg();
                Some(Elm::Frac(a, b))
            }
            "sqrt" => {
                self.skip_ws();
                let deg = if self.eat('[') {
                    self.stop = Some(']');
                    let d = self.parse_until(false, false);
                    self.stop = None;
                    self.eat(']');
                    Some(d)
                } else {
                    None
                };
                Some(Elm::Rad(deg, self.parse_arg()))
            }
            "text" | "textrm" | "mbox" => Some(Elm::Run(self.read_group_raw(), true)),
            "operatorname" => Some(Elm::Run(self.read_group_raw(), true)),
            "mathrm" | "mathbf" | "mathit" | "mathcal" | "mathbb" => {
                let mut v = self.parse_arg();
                if cmd != "mathbf" {
                    mark_upright(&mut v);
                }
                Some(Elm::Seq(v))
            }
            "left" => {
                let beg = self.parse_delim();
                let body = self.parse_until(false, true);
                let end = if self.at_right() {
                    self.i += "\\right".chars().count();
                    self.parse_delim()
                } else {
                    None
                };
                Some(Elm::Delim(beg, end, body))
            }
            "right" => {
                // 游离的 \right:吞掉其后定界符
                self.parse_delim();
                None
            }
            "vec" | "bar" | "hat" | "tilde" | "dot" | "ddot" | "overline" => {
                let mark = match cmd.as_str() {
                    "vec" => '⃗',
                    "bar" | "overline" => '¯',
                    "hat" => 'ˆ',
                    "tilde" => '˜',
                    "dot" => '˙',
                    _ => '¨',
                };
                Some(Elm::Accent(mark, self.parse_arg()))
            }
            "begin" => {
                let env = self.read_group_raw();
                if env == "array" || env == "tabular" {
                    // 列格式参数
                    self.read_group_raw();
                }
                let body = self.parse_until(false, false);
                Some(Elm::Seq(body))
            }
            "end" => {
                self.read_group_raw();
                None
            }
            "quad" | "qquad" | "enspace" | "thinspace" => Some(Elm::Run(" ".into(), true)),
            "displaystyle" | "textstyle" | "scriptstyle" | "limits" | "nolimits" => None,
            _ => {
                if let Some(sym) = symbol(&cmd) {
                    Some(Elm::Run(sym.into(), false))
                } else if let Some(word) = upright_fn(&cmd) {
                    Some(Elm::Run(format!("{word} "), true))
                } else {
                    Some(Elm::Run(cmd, true))
                }
            }
        }
    }

    fn parse_delim(&mut self) -> Option<char> {
        self.skip_ws();
        match self.peek()? {
            '.' => {
                self.i += 1;
                None
            }
            '\\' => {
                self.i += 1;
                let cmd = self.read_cmd();
                match cmd.as_str() {
                    "{" => Some('{'),
                    "}" => Some('}'),
                    "|" => Some('|'),
                    "langle" => Some('⟨'),
                    "rangle" => Some('⟩'),
                    "lceil" => Some('⌈'),
                    "rceil" => Some('⌉'),
                    "lfloor" => Some('⌊'),
                    "rfloor" => Some('⌋'),
                    _ => Some('|'),
                }
            }
            c => {
                self.i += 1;
                Some(c)
            }
        }
    }
}

fn mark_upright(v: &mut [Elm]) {
    for e in v {
        match e {
            Elm::Run(_, up) => *up = true,
            Elm::Seq(inner) | Elm::Delim(_, _, inner) | Elm::Accent(_, inner) => {
                mark_upright(inner)
            }
            Elm::Frac(a, b) => {
                mark_upright(a);
                mark_upright(b);
            }
            _ => {}
        }
    }
}
