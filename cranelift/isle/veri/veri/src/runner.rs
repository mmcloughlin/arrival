use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anyhow::{bail, format_err, Error, Result};
use cranelift_isle::{sema::TermId, trie_again::RuleSet};
use rayon::prelude::*;
use serde::Serialize;

use crate::{
    debug::print_expansion,
    expand::{Chaining, Expander, Expansion},
    program::Program,
    solver::{Applicability, Solver, Verification},
    type_inference::{self, type_constraint_system, Assignment, Choice},
    veri::Conditions,
};

const LOG_DIR: &str = ".veriisle";

pub enum SolverBackend {
    Z3,
    CVC5,
}

impl SolverBackend {
    fn prog(&self) -> &str {
        match self {
            SolverBackend::Z3 => "z3",
            SolverBackend::CVC5 => "cvc5",
        }
    }

    fn args(&self, timeout: Duration) -> Vec<String> {
        match self {
            SolverBackend::Z3 => vec![
                "-smt2".to_string(),
                "-in".to_string(),
                format!("-t:{}", timeout.as_millis()),
            ],
            SolverBackend::CVC5 => vec![
                "--incremental".to_string(),
                "--print-success".to_string(),
                format!("--tlimit-per={ms}", ms = timeout.as_millis()),
                "-".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
enum ExpansionPredicate {
    FirstRuleNamed,
    Specified,
    Tagged(String),
    Root(String),
    ContainsRule(String),
    Not(Box<ExpansionPredicate>),
    And(Box<ExpansionPredicate>, Box<ExpansionPredicate>),
}

impl FromStr for ExpansionPredicate {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(if let Some((p, q)) = s.split_once(',') {
            ExpansionPredicate::And(Box::new(p.parse()?), Box::new(q.parse()?))
        } else if let Some(p) = s.strip_prefix("not:") {
            ExpansionPredicate::Not(Box::new(p.parse()?))
        } else if s == "first-rule-named" {
            ExpansionPredicate::FirstRuleNamed
        } else if s == "specified" {
            ExpansionPredicate::Specified
        } else if let Some(tag) = s.strip_prefix("tag:") {
            ExpansionPredicate::Tagged(tag.to_string())
        } else if let Some(term) = s.strip_prefix("root:") {
            ExpansionPredicate::Root(term.to_string())
        } else if let Some(rule) = s.strip_prefix("rule:") {
            ExpansionPredicate::ContainsRule(rule.to_string())
        } else {
            bail!("invalid expansion predicate")
        })
    }
}

#[derive(Debug, Clone)]
pub struct Filter {
    include: bool,
    predicate: ExpansionPredicate,
}

impl Filter {
    fn new(include: bool, predicate: ExpansionPredicate) -> Self {
        Self { include, predicate }
    }

    fn include(predicate: ExpansionPredicate) -> Self {
        Self::new(true, predicate)
    }

    fn exclude(predicate: ExpansionPredicate) -> Self {
        Self::new(false, predicate)
    }
}

impl FromStr for Filter {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let (include, p) = if let Some(p) = s.strip_prefix("include:") {
            (true, p)
        } else if let Some(p) = s.strip_prefix("exclude:") {
            (false, p)
        } else {
            (true, s)
        };
        Ok(Filter::new(include, p.parse()?))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Inapplicable,
    Success,
    Unknown,
}

#[derive(Serialize)]
pub struct VerifyReport {
    pub verdict: Verdict,
}

#[derive(Serialize)]
pub struct TypeInstantationReport {
    pub choices: Vec<String>,
    pub verify: VerifyReport,
}

#[derive(Serialize)]
pub struct ExpansionReport {
    pub id: usize,
    pub description: String,
    pub tags: Vec<String>,
    pub type_instantiations: Vec<TypeInstantationReport>,
}

/// Runner orchestrates execution of the verification process over a set of
/// expansions.
pub struct Runner {
    prog: Program,
    term_rule_sets: HashMap<TermId, RuleSet>,

    root_term: String,
    filters: Vec<Filter>,
    solver_backend: SolverBackend,
    timeout: Duration,
    log_dir: PathBuf,
    skip_solver: bool,
    results_to_log_dir: bool,
    debug: bool,
}

impl Runner {
    pub fn from_files(inputs: &Vec<PathBuf>) -> Result<Self> {
        let expand_internal_extractors = false;
        let prog = Program::from_files(inputs, expand_internal_extractors)?;
        let term_rule_sets: HashMap<_, _> = prog.build_trie()?.into_iter().collect();
        Ok(Self {
            prog,
            term_rule_sets,
            root_term: "lower".to_string(),
            filters: Vec::new(),
            solver_backend: SolverBackend::CVC5,
            timeout: Duration::from_secs(5),
            log_dir: PathBuf::from(LOG_DIR),
            results_to_log_dir: false,
            skip_solver: false,
            debug: false,
        })
    }

    pub fn set_root_term(&mut self, term: &str) {
        self.root_term = term.to_string();
    }

    pub fn filter(&mut self, filter: Filter) {
        self.filters.push(filter);
    }

    pub fn filters(&mut self, filters: &[Filter]) {
        self.filters.extend(filters.iter().cloned());
    }

    pub fn include_first_rule_named(&mut self) {
        self.filters
            .push(Filter::include(ExpansionPredicate::FirstRuleNamed));
    }

    pub fn skip_tag(&mut self, tag: &str) {
        self.filters
            .push(Filter::exclude(ExpansionPredicate::Tagged(tag.to_string())));
    }

    pub fn target_rule(&mut self, id: &str) -> Result<()> {
        self.filters
            .push(Filter::include(ExpansionPredicate::ContainsRule(
                id.to_string(),
            )));
        Ok(())
    }

    pub fn set_solver_backend(&mut self, backend: SolverBackend) {
        self.solver_backend = backend;
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn set_log_dir(&mut self, path: PathBuf) {
        self.log_dir = path;
    }

    pub fn set_results_to_log_dir(&mut self, enabled: bool) {
        self.results_to_log_dir = enabled;
    }

    pub fn skip_solver(&mut self, skip: bool) {
        self.skip_solver = skip;
    }

    pub fn debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    pub fn run(&self) -> Result<()> {
        // Clean log directory.
        if self.log_dir.exists() {
            std::fs::remove_dir_all(&self.log_dir)?;
        }

        // Generate expansions.
        // TODO(mbm): don't hardcode the expansion configuration
        let chaining = Chaining::new(&self.prog, &self.term_rule_sets)?;
        chaining.validate()?;
        let mut expander = Expander::new(&self.prog, &self.term_rule_sets, chaining);
        expander.add_root_term_name(&self.root_term)?;
        expander.set_prune_infeasible(true);
        expander.expand();

        // Process expansions.
        let expansions = expander.expansions();
        log::info!("expansions: {n}", n = expansions.len());

        expansions
            .par_iter()
            .enumerate()
            .try_for_each(|(i, expansion)| -> Result<()> {
                // Skip?
                if !self.should_verify(expansion)? {
                    return Ok(());
                }

                // Verify
                let expansion_log_dir = self.log_dir.join(format!("{:05}", i));
                let report = self.verify_expansion(expansion, i, expansion_log_dir.clone())?;

                // Write
                let output = Self::open_log_file(expansion_log_dir.clone(), "report.json")?;
                serde_json::to_writer_pretty(output, &report)?;

                Ok(())
            })?;

        Ok(())
    }

    fn should_verify(&self, expansion: &Expansion) -> Result<bool> {
        let mut verdict = None;
        for filter in self.filters.iter() {
            verdict = self.eval_filter(filter, expansion)?.or(verdict);
        }
        Ok(verdict.unwrap_or(false))
    }

    fn eval_filter(&self, filter: &Filter, expansion: &Expansion) -> Result<Option<bool>> {
        Ok(if self.eval_predicate(&filter.predicate, expansion)? {
            Some(filter.include)
        } else {
            None
        })
    }

    fn eval_predicate(
        &self,
        predicate: &ExpansionPredicate,
        expansion: &Expansion,
    ) -> Result<bool> {
        Ok(match predicate {
            ExpansionPredicate::FirstRuleNamed => {
                let rule_id = expansion
                    .rules
                    .first()
                    .ok_or(format_err!("expansion should have at least one rule"))?;
                let rule = self.prog.rule(*rule_id);
                rule.name.is_some()
            }
            ExpansionPredicate::Specified => expansion
                .terms(&self.prog)
                .iter()
                .all(|term_id| self.prog.specenv.has_spec(*term_id)),
            ExpansionPredicate::Tagged(tag) => {
                let tags = expansion.tags(&self.prog);
                tags.contains(tag)
            }
            ExpansionPredicate::Root(term) => self.prog.term_name(expansion.term) == term,
            ExpansionPredicate::ContainsRule(identifier) => {
                let rule = self
                    .prog
                    .get_rule_by_identifier(&identifier)
                    .ok_or(format_err!("unknown rule '{identifier}'"))?;
                expansion.rules.contains(&rule.id)
            }
            ExpansionPredicate::Not(p) => !self.eval_predicate(p, expansion)?,
            ExpansionPredicate::And(p, q) => {
                self.eval_predicate(p, expansion)? && self.eval_predicate(q, expansion)?
            }
        })
    }

    fn verify_expansion(
        &self,
        expansion: &Expansion,
        id: usize,
        log_dir: std::path::PathBuf,
    ) -> Result<ExpansionReport> {
        let description = self.expansion_description(expansion)?;

        // Results output.
        let mut output: Box<dyn Write> = if self.results_to_log_dir {
            log::info!("#{id}\t{description}");
            Box::new(Self::open_log_file(log_dir.clone(), "results.out")?)
        } else {
            Box::new(std::io::stdout())
        };

        writeln!(output, "#{id}\t{}", self.expansion_description(expansion)?)?;
        if self.debug {
            print_expansion(&self.prog, expansion);
        }

        // Verification conditions.
        let conditions = Conditions::from_expansion(expansion, &self.prog)?;
        if self.debug {
            conditions.pretty_print(&self.prog);
        }

        // Type constraints.
        let system = type_constraint_system(&conditions);
        if self.debug {
            system.pretty_print();
        }

        // Infer types.
        let type_solver = type_inference::Solver::new();
        let solutions = type_solver.solve(&system);

        // Initialize report.
        let mut tags: Vec<_> = expansion.tags(&self.prog).iter().cloned().collect();
        tags.sort();
        let mut report = ExpansionReport {
            id,
            description,
            tags,
            type_instantiations: Vec::new(),
        };

        for (i, solution) in solutions.iter().enumerate() {
            // Show type assignment.
            let mut choices = Vec::new();
            for choice in &solution.choices {
                let choice = match choice {
                    Choice::TermInstantiation(term_id, sig) => {
                        format!("{term}{sig}", term = self.prog.term_name(*term_id))
                    }
                };
                writeln!(output, "\t{choice}")?;
                choices.push(choice);
            }
            writeln!(output, "\t\ttype solution status = {}", solution.status)?;
            if self.debug {
                println!("type assignment:");
                solution.assignment.pretty_print(&conditions);
            }

            match &solution.status {
                type_inference::Status::Solved => (),
                type_inference::Status::Inapplicable(conflict) => {
                    log::debug!(
                        "inapplicable type inference: {diagnostic}",
                        diagnostic = conflict.diagnostic(&conditions, &self.prog.files)
                    );
                    continue;
                }
                type_inference::Status::Underconstrained => {
                    bail!("underconstrained type inference")
                }
                type_inference::Status::TypeError(confict) => {
                    return Err(conditions.error_at_expr(
                        &self.prog,
                        confict.x,
                        confict.reason.clone(),
                    ));
                }
            }

            // Verify.
            if self.skip_solver {
                println!("skip solver");
                continue;
            }

            let solution_log_dir = log_dir.join(format!("{:03}", i));
            let verify_report = self.verify_expansion_type_instantiation(
                &conditions,
                &solution.assignment,
                solution_log_dir,
                &mut output,
            )?;

            // Append to report.
            report.type_instantiations.push(TypeInstantationReport {
                choices,
                verify: verify_report,
            });
        }

        Ok(report)
    }

    fn verify_expansion_type_instantiation(
        &self,
        conditions: &Conditions,
        assignment: &Assignment,
        log_dir: std::path::PathBuf,
        output: &mut dyn Write,
    ) -> Result<VerifyReport> {
        // Solve.
        let binary = self.solver_backend.prog();
        let args = self.solver_backend.args(self.timeout);
        let replay_file = Self::open_log_file(log_dir, "solver.smt2")?;
        let smt = easy_smt::ContextBuilder::new()
            .solver(binary, &args)
            .replay_file(Some(replay_file))
            .build()?;

        let mut solver = Solver::new(smt, &self.prog, conditions, assignment)?;
        solver.encode()?;

        let applicability = solver.check_assumptions_feasibility()?;
        writeln!(output, "\t\tapplicability = {applicability}")?;
        match applicability {
            Applicability::Applicable => (),
            Applicability::Inapplicable => {
                return Ok(VerifyReport {
                    verdict: Verdict::Inapplicable,
                })
            }
            Applicability::Unknown => bail!("could not prove applicability"),
        };

        let verification = solver.check_verification_condition()?;
        writeln!(output, "\t\tverification = {verification}")?;
        Ok(match verification {
            Verification::Failure(model) => {
                println!("model:");
                conditions.print_model(&model, &self.prog)?;
                bail!("verification failed");
            }
            Verification::Success => VerifyReport {
                verdict: Verdict::Success,
            },
            Verification::Unknown => VerifyReport {
                verdict: Verdict::Unknown,
            },
        })
    }

    /// Human-readable description of an expansion.
    fn expansion_description(&self, expansion: &Expansion) -> Result<String> {
        let rule_id = expansion
            .rules
            .first()
            .ok_or(format_err!("expansion should have at least one rule"))?;
        let rule = self.prog.rule(*rule_id);
        Ok(rule.identifier(&self.prog.tyenv, &self.prog.files))
    }

    fn open_log_file<P: AsRef<Path>>(log_dir: std::path::PathBuf, name: P) -> Result<File> {
        std::fs::create_dir_all(&log_dir)?;
        let path = log_dir.join(name);
        let file = File::create(&path)?;
        Ok(file)
    }
}
