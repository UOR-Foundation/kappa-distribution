//! TestRunner - test execution, verdict tracking, and terminal output.

use crate::client::HttpTrace;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Must,
    Should,
    May,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVerdict {
    Pass,
    Fail,
    Warn,
    Skip,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub id: &'static str,
    pub level: u8,
    pub requirement: Requirement,
    pub description: &'static str,
    pub verdict: TestVerdict,
    pub message: Option<String>,
    pub traces: Vec<HttpTrace>,
    pub duration_ms: u64,
}

pub struct TestRunner {
    pub results: Vec<TestResult>,
}

impl TestRunner {
    pub fn new() -> Self {
        TestRunner {
            results: Vec::new(),
        }
    }

    pub fn must(
        &mut self,
        id: &'static str,
        level: u8,
        description: &'static str,
    ) -> TestContext<'_> {
        TestContext::new(self, id, level, Requirement::Must, description)
    }

    pub fn should(
        &mut self,
        id: &'static str,
        level: u8,
        description: &'static str,
    ) -> TestContext<'_> {
        TestContext::new(self, id, level, Requirement::Should, description)
    }

    pub fn may(
        &mut self,
        id: &'static str,
        level: u8,
        description: &'static str,
    ) -> TestContext<'_> {
        TestContext::new(self, id, level, Requirement::May, description)
    }

    pub fn skip(&mut self, id: &'static str, level: u8, description: &'static str, reason: &str) {
        print_verdict(id, TestVerdict::Skip, description, Some(reason));
        self.results.push(TestResult {
            id,
            level,
            requirement: Requirement::Must,
            description,
            verdict: TestVerdict::Skip,
            message: Some(reason.to_string()),
            traces: Vec::new(),
            duration_ms: 0,
        });
    }

    pub fn summary(&self) -> (usize, usize, usize, usize) {
        let (mut p, mut f, mut w, mut s) = (0, 0, 0, 0);
        for r in &self.results {
            match r.verdict {
                TestVerdict::Pass => p += 1,
                TestVerdict::Fail => f += 1,
                TestVerdict::Warn => w += 1,
                TestVerdict::Skip => s += 1,
            }
        }
        (p, f, w, s)
    }

    pub fn has_failures(&self) -> bool {
        self.results.iter().any(|r| r.verdict == TestVerdict::Fail)
    }
}

pub struct TestContext<'a> {
    id: &'static str,
    level: u8,
    requirement: Requirement,
    description: &'static str,
    traces: Vec<HttpTrace>,
    runner: &'a mut TestRunner,
    start: Instant,
}

impl<'a> TestContext<'a> {
    fn new(
        runner: &'a mut TestRunner,
        id: &'static str,
        level: u8,
        requirement: Requirement,
        description: &'static str,
    ) -> Self {
        TestContext {
            id,
            level,
            requirement,
            description,
            traces: Vec::new(),
            runner,
            start: Instant::now(),
        }
    }

    pub fn trace(&mut self, t: HttpTrace) {
        self.traces.push(t);
    }

    pub fn pass(self) {
        self.finish(TestVerdict::Pass, None);
    }

    pub fn fail(self, message: String) {
        self.finish(TestVerdict::Fail, Some(message));
    }

    pub fn warn(self, message: String) {
        self.finish(TestVerdict::Warn, Some(message));
    }

    fn finish(self, mut verdict: TestVerdict, message: Option<String>) {
        let duration_ms = self.start.elapsed().as_millis() as u64;

        // Downgrade: Should+Fail to Warn, May+Fail to Warn.
        if verdict == TestVerdict::Fail {
            match self.requirement {
                Requirement::Should | Requirement::May => verdict = TestVerdict::Warn,
                Requirement::Must => {}
            }
        }

        print_verdict(self.id, verdict, self.description, message.as_deref());

        self.runner.results.push(TestResult {
            id: self.id,
            level: self.level,
            requirement: self.requirement,
            description: self.description,
            verdict,
            message,
            traces: self.traces,
            duration_ms,
        });
    }
}

fn print_verdict(id: &str, verdict: TestVerdict, description: &str, message: Option<&str>) {
    let is_tty = std::env::var("TERM").is_ok();
    let (tag, color, reset) = match verdict {
        TestVerdict::Pass => (
            "PASS",
            if is_tty { "\x1b[32m" } else { "" },
            if is_tty { "\x1b[0m" } else { "" },
        ),
        TestVerdict::Fail => (
            "FAIL",
            if is_tty { "\x1b[31m" } else { "" },
            if is_tty { "\x1b[0m" } else { "" },
        ),
        TestVerdict::Warn => (
            "WARN",
            if is_tty { "\x1b[33m" } else { "" },
            if is_tty { "\x1b[0m" } else { "" },
        ),
        TestVerdict::Skip => (
            "SKIP",
            if is_tty { "\x1b[90m" } else { "" },
            if is_tty { "\x1b[0m" } else { "" },
        ),
    };
    match message {
        Some(msg) => eprintln!("{color}[{tag}]{reset}  {id}  {description} - {msg}"),
        None => eprintln!("{color}[{tag}]{reset}  {id}  {description}"),
    }
}
