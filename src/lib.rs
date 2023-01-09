#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
use log::{debug, error, info, warn};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::types::{PyDict, PyFunction};
use std::any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::io::Write;
use std::rc::Rc;
use std::sync::Once;
use std::thread::current; // Use log crate when building application

use thiserror::Error;

use pyo3::prelude::*;

mod engine;
#[cfg(test)]
mod tests;

use engine::{JobKind, PPGEvaluator};

static LOGGER_INIT: Once = Once::new();

#[derive(Error, Debug)]
pub enum PPGEvaluatorError {
    #[error("API error. You're holding it wrong")]
    APIError(String),
    #[error("Ephemeral was validated, but rerun for downstreams. It changed output, violating the constant input->constant output contstraint.")]
    EphemeralChangedOutput,
}
pub trait PPGEvaluatorStrategy {
    fn output_already_present(&self, query: &str) -> bool;
    fn is_history_altered(
        &self,
        job_id_upstream: &str,
        job_id_downstream: &str,
        last_recorded_value: &str,
        current_value: &str,
    ) -> bool;
}

#[derive(Clone)]
pub struct StrategyForTesting {
    pub already_done: Rc<RefCell<HashSet<String>>>,
}

impl StrategyForTesting {
    pub fn new() -> Self {
        StrategyForTesting {
            already_done: Rc::new(RefCell::new(HashSet::new())),
        }
    }
}

impl PPGEvaluatorStrategy for StrategyForTesting {
    fn output_already_present(&self, query: &str) -> bool {
        self.already_done.borrow().contains(query)
    }

    fn is_history_altered(
        &self,
        job_id_upstream: &str,
        job_id_downstream: &str,
        last_recorded_value: &str,
        current_value: &str,
    ) -> bool {
        last_recorded_value != current_value
    }
}

fn start_logging() {
    let start_time = chrono::Utc::now();
    if !LOGGER_INIT.is_completed() {
        LOGGER_INIT.call_once(move || {
            use colored::Colorize;
            let start_time2 = start_time.clone();
            env_logger::builder()
                .format(move |buf, record| {
                    let filename = record
                        .file()
                        .unwrap_or("unknown")
                        .trim_start_matches("src/");
                    let ff = format!("{}:{}", filename, record.line().unwrap_or(0));
                    let ff = match record.level() {
                        log::Level::Error => ff.red(),
                        log::Level::Warn => ff.yellow(),
                        log::Level::Info => ff.blue(),
                        log::Level::Debug => ff.green(),
                        log::Level::Trace => ff.normal(),
                    };

                    writeln!(
                        buf,
                        "{}\t{:.4} | {}",
                        ff,
                        (chrono::Utc::now() - start_time2).num_milliseconds(),
                        //chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                        record.args()
                    )
                })
                .is_test(true)
                .init()
        });
    }
}

// simulates a complete (deterministic)
// run - jobs just register that they've been run,
// and output a 'dummy' history.
pub struct TestGraphRunner {
    pub setup_graph: Box<dyn Fn(&mut PPGEvaluator<StrategyForTesting>)>,
    pub run_counters: HashMap<String, usize>,
    pub history: HashMap<String, String>,
    pub already_done: HashSet<String>,
    pub allowed_nesting: u32,
    pub outputs: HashMap<String, String>,
    pub run_order: Vec<String>,
}

impl TestGraphRunner {
    pub fn new(setup_func: Box<dyn Fn(&mut PPGEvaluator<StrategyForTesting>)>) -> Self {
        TestGraphRunner {
            setup_graph: setup_func,
            run_counters: HashMap::new(),
            history: HashMap::new(),
            already_done: HashSet::new(),
            allowed_nesting: 250,
            outputs: HashMap::new(),
            run_order: Vec::new(),
        }
    }

    pub fn run(
        &mut self,
        jobs_to_fail: &[&str],
    ) -> Result<PPGEvaluator<StrategyForTesting>, PPGEvaluatorError> {
        let strat = StrategyForTesting::new();
        for k in self.already_done.iter() {
            strat.already_done.borrow_mut().insert(k.to_string());
        }
        let already_done2 = Rc::clone(&strat.already_done);
        let mut g = PPGEvaluator::new_with_history(self.history.clone(), strat);
        self.run_order.clear();

        (self.setup_graph)(&mut g);
        let mut counter = self.allowed_nesting;
        g.event_startup().unwrap();
        while !g.is_finished() {
            let to_run = g.query_ready_to_run();
            assert!(!to_run.is_empty());
            for job_id in to_run.iter() {
                debug!("Running {}", job_id);
                g.event_now_running(job_id)?;
                self.run_order.push(job_id.to_string());
                *self.run_counters.entry(job_id.clone()).or_insert(0) += 1;
                if jobs_to_fail.contains(&&job_id[..]) {
                    g.event_job_finished_failure(job_id).unwrap();
                } else {
                    match g.event_job_finished_success(
                        job_id,
                        self.outputs
                            .get(job_id)
                            .unwrap_or(&format!("history_{}", job_id))
                            .to_string(),
                    ) {
                        Ok(_) => {}
                        Err(err) => match err {
                            PPGEvaluatorError::APIError(x) => panic!("api error {}", x),
                            PPGEvaluatorError::EphemeralChangedOutput => {
                                debug!("EphemeralChangedOutput error. ignoring for tests");
                            }
                        },
                    }
                }
                already_done2.borrow_mut().insert(job_id.clone());
            }
            counter -= 1;
            if counter == 0 {
                return Err(PPGEvaluatorError::APIError(format!(
                    "run out of room, you nested them more than {} deep?",
                    self.allowed_nesting
                )));
            }
        }
        self.history.clear();
        for (k, v) in g.new_history().iter() {
            self.history.insert(k.clone(), v.clone());
        }
        for k in already_done2.take().into_iter() {
            self.already_done.insert(k);
        }
        if !g.verify_order_was_topological(&self.run_order) {
            panic!("Run order was not topological");
        }

        Ok(g)
    }

    fn assert_run_order_is_topological(&self, g: &PPGEvaluator<StrategyForTesting>) {}
}

pub fn test_big_linear_graph(count: u32) {
    let c2 = count;
    let create_graph = move |g: &mut PPGEvaluator<StrategyForTesting>| {
        let c = c2 - 1;
        for ii in 0..c {
            g.add_node(&format!("A{}", ii), JobKind::Output);
        }
        for ii in 1..c {
            g.depends_on(&format!("A{}", ii - 1), &format!("A{}", ii));
        }
    };
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    ro.allowed_nesting = count + 1;
    let g = ro.run(&Vec::new());
    assert!(g.is_ok())
    //dbg!(g.new_history().len());
}

pub fn test_big_linear_graph_half_ephemeral(count: u32) {
    let c2 = count;
    let create_graph = move |g: &mut PPGEvaluator<StrategyForTesting>| {
        let c = c2 - 1;
        for ii in 0..c {
            g.add_node(
                &format!("A{}", ii),
                if ii % 2 == 0 {
                    JobKind::Output
                } else {
                    JobKind::Ephemeral
                },
            );
        }
        for ii in 1..c {
            g.depends_on(&format!("A{}", ii - 1), &format!("A{}", ii));
        }
    };
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    ro.allowed_nesting = count + 1;
    let g = ro.run(&Vec::new());
    assert!(g.is_ok())
    //dbg!(g.new_history().len());
}

struct StrategyForPython {
    history_altered_callback: PyObject,
}

impl PPGEvaluatorStrategy for StrategyForPython {
    fn output_already_present(&self, query: &str) -> bool {
        use std::path::PathBuf;
        let p = PathBuf::from(query);
        p.exists()
    }

    fn is_history_altered(
        &self,
        job_id_upstream: &str,
        job_id_downstream: &str,
        last_recorded_value: &str,
        current_value: &str,
    ) -> bool {
        if last_recorded_value == current_value {
            false
        } else {
            Python::with_gil(|py| {
                let res = self.history_altered_callback.call1(
                    py,
                    (
                        job_id_upstream,
                        job_id_downstream,
                        last_recorded_value,
                        current_value,
                    ),
                );
                res.expect("History comparison failed on python side")
                    .extract::<bool>(py)
                    .expect("history comparison did not return a bool")
            })
        }

        //last_recorded_value != current_value // todo
    }
}

#[pyclass(name = "PPG2Evaluator")]
pub struct PyPPG2Evaluator {
    evaluator: PPGEvaluator<StrategyForPython>, // todo
}

impl From<PPGEvaluatorError> for PyErr {
    fn from(val: PPGEvaluatorError) -> Self {
        PyValueError::new_err(val.to_string())
    }
}

#[pymethods]
impl PyPPG2Evaluator {
    #[new]
    fn __new__(
        py: Python,
        py_history: &PyDict,
        history_compare_callable: PyObject,
    ) -> Result<Self, PyErr> {
        let mut history: HashMap<String, String> = HashMap::new();
        for (k, v) in py_history.iter() {
            let ko: String = k.extract()?;
            let vo: String = v.extract()?;
            history.insert(ko, vo);
        }
        Ok(PyPPG2Evaluator {
            evaluator: PPGEvaluator::new_with_history(
                history,
                StrategyForPython {
                    history_altered_callback: history_compare_callable,
                },
            ),
        })
    }

    pub fn add_node(&mut self, job_id: &str, job_kind: &str) -> Result<(), PyErr> {
        let jk = match job_kind {
            "Output" => JobKind::Output,
            "Always" => JobKind::Always,
            "Ephemeral" => JobKind::Ephemeral,
            _ => return Err(PyTypeError::new_err("Invalid job kind")),
        };
        self.evaluator.add_node(job_id, jk);
        Ok(())
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.evaluator.depends_on(from, to);
    }

    pub fn event_startup(&mut self) -> Result<(), PyErr> {
        Ok(self.evaluator.event_startup()?)
    }

    pub fn event_now_running(&mut self, job_id: &str) -> Result<(), PyErr> {
        Ok(self.evaluator.event_now_running(job_id)?)
    }

    pub fn event_job_success(&mut self, job_id: &str, new_history: &str) -> Result<(), PyErr> {
        Ok(self
            .evaluator
            .event_job_finished_success(job_id, new_history.to_string())?)
    }

    pub fn event_job_failure(&mut self, job_id: &str) -> Result<(), PyErr> {
        Ok(self.evaluator.event_job_finished_failure(job_id)?)
    }

    pub fn list_upstream_failed_jobs(&self) -> Vec<String> {
        self.evaluator.query_upstream_failed().into_iter().collect()
    }

    pub fn jobs_ready_to_run(&self) -> Vec<String> {
        self.evaluator.query_ready_to_run().into_iter().collect()
    }

    pub fn jobs_ready_for_cleanup(&self) -> Vec<String> {
        self.evaluator
            .query_ready_for_cleanup()
            .into_iter()
            .collect()
    }

    pub fn event_job_cleanup_done(&mut self, job_id: &str) -> Result<(), PyErr> {
        Ok(self.evaluator.event_job_cleanup_done(job_id)?)
    }

    pub fn is_finished(&self) -> bool {
        self.evaluator.is_finished()
    }

    pub fn new_history(&self) -> HashMap<String, String> {
        self.evaluator.new_history()
    }
}

/// Formats the sum of two numbers as string.
#[pyfunction]
fn enable_logging() -> PyResult<()> {
    start_logging();
    error!("hello from rust");
    Ok(())
}

/// A Python module implemented in Rust.
#[pymodule]
fn pypipegraph2(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(enable_logging, m)?)?;
    m.add_class::<PyPPG2Evaluator>()?;
    Ok(())
}
