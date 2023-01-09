/* ↓ ←➔ ↑ */
#![allow(unused_macros)]
use std::collections::{HashMap, HashSet};
use std::{cell::RefCell, rc::Rc};

use crate::*;

macro_rules! set {
    ( $( $x:expr ),* ) => {  // Match zero or more comma delimited items
        {
            let mut temp_set = HashSet::new();  // Create a mutable HashSet
            $(
                temp_set.insert($x.to_string()); // Insert each item matched into the HashSet
            )*
            temp_set // Return the populated HashSet
        }
    };
}
#[test]
fn test_one_output() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Output);
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    dbg!(&new_history);
    assert!(new_history.contains_key("A"));
    assert!(ro.run_counters.get("A") == Some(&1));
    error!("Run again");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
}

#[test]
pub fn test_three_outputs() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("out", JobKind::Output);
    g.add_node("out2", JobKind::Output);
    g.add_node("out3", JobKind::Output);
    g.depends_on("out2", "out");
    g.depends_on("out3", "out");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["out"]);
    g.event_now_running("out").unwrap();
    assert!(g.query_ready_to_run().is_empty());
    g.event_job_finished_success("out", "outAResult".to_string())
        .unwrap();
    assert_eq!(g.query_ready_to_run(), set!["out2", "out3"]);
    assert!(!g.is_finished());
    g.event_now_running("out2").unwrap();
    g.event_now_running("out3").unwrap();
    assert!(!g.is_finished());
    g.event_job_finished_success("out2", "out2output".to_string())
        .unwrap();
    assert!(!g.is_finished());
    g.event_job_finished_success("out3", "out3output".to_string())
        .unwrap();
    assert!(g.is_finished());
    let history = g.new_history();
    dbg!(&history);
    assert!(history.get(&("out!!!out2".to_string())).unwrap() == "outAResult");
    assert!(history.get(&("out!!!out3".to_string())).unwrap() == "outAResult");
    dbg!(&history);
    assert!(history.len() == 2 + 3);

    // ok, but the history is not out2-> None, out3->None.
    // When I add a job, and I don't need to rerun out,
    //
    //assert!(history.get(&("out".to_string(), "out2".to_string())).unwrap() == "outAResult")
}

#[test]
#[should_panic]
pub fn test_simple_cycle() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("out", JobKind::Output);
    g.depends_on("out", "out");
}

#[test]
pub fn test_failure() {
    start_logging();
    let mut his = HashMap::new();
    his.insert("Job_not_present".to_string(), "hello".to_string());
    let mut g = PPGEvaluator::new_with_history(his, StrategyForTesting::new());
    g.add_node("out", JobKind::Output);
    g.add_node("out2", JobKind::Output);
    g.add_node("out3", JobKind::Output);
    g.depends_on("out2", "out");
    g.depends_on("out3", "out2");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["out"]);
    g.event_now_running("out").unwrap();
    assert!(g.query_ready_to_run().is_empty());
    g.event_job_finished_failure("out").unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.is_finished());
    //we keep history that for jobs tha are currently not present
    assert!(g.new_history().len() == 1);
    assert!(g.new_history().get("Job_not_present").is_some())
}

#[test]
pub fn test_job_already_done() {
    let strat = StrategyForTesting::new();
    let mut his = HashMap::new();
    strat.already_done.borrow_mut().insert("out".to_string());
    his.insert("out".to_string(), "out".to_string());
    let mut g = PPGEvaluator::new_with_history(his, strat);
    g.add_node("out", JobKind::Output);
    g.event_startup().unwrap();
    assert!(g.is_finished());
}

#[test]
pub fn simplest_ephemeral() {
    start_logging();
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("out", JobKind::Output);
    g.add_node("in", JobKind::Ephemeral);
    g.depends_on("out", "in");
    g.event_startup().unwrap();
    assert!(!g.is_finished());
    assert_eq!(g.query_ready_to_run(), set!["in"]);
    g.event_now_running("in").unwrap();
    g.event_job_finished_success("in", "".to_string()).unwrap();

    assert!(!g.is_finished());
    assert_eq!(g.query_ready_to_run(), set!["out"]);
    dbg!(g.query_ready_for_cleanup());
    assert!(g.query_ready_for_cleanup().is_empty());

    g.event_now_running("out").unwrap();
    g.event_job_finished_success("out", "".to_string()).unwrap();
    assert!(g.is_finished());
    assert_eq!(g.query_ready_for_cleanup(), set!["in"]);
    assert!(g.query_ready_to_run().is_empty())
}

#[test]
pub fn ephemeral_output_already_done() {
    start_logging();
    let mut his = HashMap::new();
    his.insert("in!!!out".to_string(), "".to_string());
    his.insert("in".to_string(), "".to_string());
    his.insert("out".to_string(), "".to_string());
    let strat = StrategyForTesting::new();
    strat.already_done.borrow_mut().insert("out".to_string());
    let mut g = PPGEvaluator::new_with_history(his, strat);
    g.add_node("out", JobKind::Output);
    g.add_node("in", JobKind::Ephemeral);
    g.depends_on("out", "in");
    g.event_startup().unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.is_finished());
}
#[test]
pub fn ephemeral_nested() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("A", JobKind::Output);
    g.add_node("B", JobKind::Ephemeral);
    g.add_node("C", JobKind::Output);
    g.add_node("D", JobKind::Ephemeral);
    g.add_node("E", JobKind::Output);
    g.depends_on("E", "D");
    g.depends_on("D", "C");
    g.depends_on("C", "B");
    g.depends_on("B", "A");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["A"]);
    assert!(!g.is_finished());

    g.event_now_running("A").unwrap();
    g.event_job_finished_success("A", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["B"]);
    assert!(!g.is_finished());

    g.event_now_running("B").unwrap();
    g.event_job_finished_success("B", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["C"]);
    assert!(!g.is_finished());

    g.event_now_running("C").unwrap();
    g.event_job_finished_success("C", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["D"]);
    assert!(!g.is_finished());

    g.event_now_running("D").unwrap();
    g.event_job_finished_success("D", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["E"]);
    assert!(!g.is_finished());

    g.event_now_running("E").unwrap();
    g.event_job_finished_success("E", "".to_string()).unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.is_finished());
}

#[test]
pub fn ephemeral_nested_first_already_present() {
    let strat = StrategyForTesting::new();
    strat.already_done.borrow_mut().insert("A".to_string());
    let mut g = PPGEvaluator::new_with_history(
        mk_history(&[
            (("E", "D"), ""),
            (("D", "C"), ""),
            (("C", "B"), ""),
            (("B", "A"), ""),
        ]),
        strat,
    );
    //dbg!(&g.history);
    g.add_node("A", JobKind::Output);
    g.add_node("B", JobKind::Ephemeral);
    g.add_node("C", JobKind::Output);
    g.add_node("D", JobKind::Ephemeral);
    g.add_node("E", JobKind::Output);
    g.depends_on("E", "D");
    g.depends_on("D", "C");
    g.depends_on("C", "B");
    g.depends_on("B", "A");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["B"]);
    assert!(!g.is_finished());

    g.event_now_running("B").unwrap();
    g.event_job_finished_success("B", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["C"]);
    assert!(!g.is_finished());

    g.event_now_running("C").unwrap();
    g.event_job_finished_success("C", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["D"]);
    assert!(!g.is_finished());

    g.event_now_running("D").unwrap();
    g.event_job_finished_success("D", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["E"]);
    assert!(!g.is_finished());

    g.event_now_running("E").unwrap();
    g.event_job_finished_success("E", "".to_string()).unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.is_finished());
}
#[test]
pub fn ephemeral_nested_last() {
    /* ↓ ←➔ ↑
    *
    *
    Eo ←  De ← Co ← Be ← Ao


    E and C Are done.
    History is available for everything.
    We need to run A (which is missing it's output,
    but does not invalidate B.)

    * */
    let strat = StrategyForTesting::new();
    strat.already_done.borrow_mut().insert("E".to_string());
    strat.already_done.borrow_mut().insert("C".to_string());
    let mut g = PPGEvaluator::new_with_history(
        mk_history(&[
            (("B", "A"), ""),
            (("C", "B"), ""),
            (("D", "C"), ""),
            (("E", "D"), ""),
            //(("F", "E"), ""),
        ]),
        strat,
    );
    g.add_node("A", JobKind::Output);
    g.add_node("B", JobKind::Ephemeral);
    g.add_node("C", JobKind::Output);
    g.add_node("D", JobKind::Ephemeral);
    g.add_node("E", JobKind::Output);
    g.depends_on("E", "D");
    g.depends_on("D", "C");
    g.depends_on("C", "B");
    g.depends_on("B", "A");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["A"]);
    assert!(!g.is_finished());

    g.event_now_running("A").unwrap();
    g.event_job_finished_success("A", "".to_string()).unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.is_finished());
}

fn mk_history(input: &[((&str, &str), &str)]) -> HashMap<String, String> {
    let mut res: HashMap<String, String> = input
        .iter()
        .map(|((downstream, upstream), c)| {
            (format!("{}!!!{}", upstream, downstream), c.to_string())
        })
        .collect();
    res.extend(
        input
            .iter()
            .map(|((a, b), c)| (b.to_string(), c.to_string())),
    );
    res
}

#[test]
pub fn ephemeral_nested_inner() {
    let strat = StrategyForTesting::new();
    strat.already_done.borrow_mut().insert("C".to_string());
    let mut g = PPGEvaluator::new_with_history(
        mk_history(&[
            (("B", "A"), ""),
            (("D", "C"), ""),
            (("C", "B"), ""),
            (("B", "A"), ""),
        ]),
        strat,
    );
    g.add_node("A", JobKind::Output);
    g.add_node("B", JobKind::Ephemeral);
    g.add_node("C", JobKind::Output);
    g.add_node("D", JobKind::Ephemeral);
    g.add_node("E", JobKind::Output);
    g.depends_on("E", "D");
    g.depends_on("D", "C");
    g.depends_on("C", "B");
    g.depends_on("B", "A");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["A"]); // this changes with the 'ephemerals cant invalidate' rule. Case can invalidate, I presume
    assert!(!g.is_finished());

    g.event_now_running("A").unwrap();
    g.event_job_finished_success("A", "".to_string()).unwrap();
    assert!(!g.is_finished());
    // C was done. B only runs if C needs to be made, or if B is invalidated
    // but A's output didn't change, B did not get invalidated, and therefore,
    // B can not invalidate C.
    // But D is needed by E, which is a missing output.
    assert_eq!(g.query_ready_to_run(), set!["D"]);

    g.event_now_running("D").unwrap();
    g.event_job_finished_success("D", "".to_string()).unwrap();
    assert!(!g.is_finished());

    g.event_now_running("E").unwrap();
    g.event_job_finished_failure("E").unwrap();

    assert!(g.query_ready_to_run().is_empty());
    assert_eq!(g.query_failed(), set!["E"]);
    assert!(g.query_upstream_failed().is_empty());
}
#[test]
pub fn ephemeral_nested_upstream_failure() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("B", JobKind::Ephemeral);
    g.add_node("D", JobKind::Ephemeral);
    g.add_node("C", JobKind::Output);
    g.add_node("E", JobKind::Output);
    g.add_node("A", JobKind::Output);
    g.depends_on("B", "A");
    g.depends_on("E", "D");
    g.depends_on("D", "C");
    g.depends_on("C", "B");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["A"]);
    assert!(!g.is_finished());

    g.event_now_running("A").unwrap();
    g.event_job_finished_failure("A").unwrap();
    assert!(g.is_finished());

    assert_eq!(g.query_failed(), set!["A"]);
    assert_eq!(g.query_upstream_failed(), set!["B", "C", "D", "E"]);
}

#[test]
pub fn disjoint_and_twice() {
    let strat = StrategyForTesting::new();
    let already_done = strat.already_done.clone();
    let init = |history| {
        let strat = strat.clone();
        let mut g = PPGEvaluator::new_with_history(history, strat);
        g.add_node("A", JobKind::Output);
        g.add_node("B", JobKind::Output);
        g.add_node("C", JobKind::Output);
        g.depends_on("B", "A");
        g.depends_on("C", "B");
        g.add_node("d", JobKind::Output);
        g.add_node("e", JobKind::Output);
        g.depends_on("d", "e");
        g
    };

    error!("part 1");
    let mut g = init(HashMap::new());

    let history = {
        g.event_startup().unwrap();
        assert_eq!(g.query_ready_to_run(), set!["A", "e"]);
        g.event_now_running("A").unwrap();
        assert_eq!(g.query_ready_to_run(), set!["e"]);
        g.event_now_running("e").unwrap();
        assert!(g.query_ready_to_run().is_empty());
        g.event_job_finished_success("e", "histe".to_string())
            .unwrap();
        already_done.borrow_mut().insert("e".to_string());
        assert_eq!(g.query_ready_to_run(), set!["d"]);
        g.event_now_running("d").unwrap();
        g.event_job_finished_success("d", "histd".to_string())
            .unwrap();
        already_done.borrow_mut().insert("d".to_string());
        assert!(g.query_ready_to_run().is_empty());
        g.event_job_finished_success("A", "histA".to_string())
            .unwrap();
        already_done.borrow_mut().insert("A".to_string());
        assert_eq!(g.query_ready_to_run(), set!["B"]);
        g.event_now_running("B").unwrap();
        assert!(g.query_ready_to_run().is_empty());
        g.event_job_finished_success("B", "histB".to_string())
            .unwrap();
        already_done.borrow_mut().insert("B".to_string());
        assert_eq!(g.query_ready_to_run(), set!["C"]);
        g.event_now_running("C").unwrap();
        assert!(g.query_ready_to_run().is_empty());
        g.event_job_finished_success("C", "histC".to_string())
            .unwrap();
        already_done.borrow_mut().insert("C".to_string());
        g.new_history()
    };
    error!("part 2");

    {
        let mut g2 = init(history.clone());
        g2.event_startup().unwrap();
        assert!(g2.is_finished());
    }

    error!("part 3");
    {
        already_done.borrow_mut().remove("C");
        let mut g2 = init(history.clone());
        g2.event_startup().unwrap();
        assert!(!g2.is_finished());
        assert_eq!(g2.query_ready_to_run(), set!["C"]);
        g2.event_now_running("C").unwrap();
        g2.event_job_finished_success("C", "histC".to_string())
            .unwrap();
        assert!(g2.is_finished());
    }
    debug!("part 4");
    {
        already_done.borrow_mut().insert("C".to_string());
        already_done.borrow_mut().remove("A");
        let mut g2 = init(history.clone());
        g2.event_startup().unwrap();
        assert!(!g2.is_finished());
        assert_eq!(g2.query_ready_to_run(), set!["A"]);
        g2.event_now_running("A").unwrap();
        g2.event_job_finished_success("A", "histA2".to_string())
            .unwrap();
        assert_eq!(g2.query_ready_to_run(), set!["B"]); // A history changed
        g2.event_now_running("B").unwrap();
        g2.event_job_finished_success("B", "histB".to_string())
            .unwrap(); // but b not changed
        assert!(g2.is_finished());
    }
}

#[test]
fn cant_start_twice() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    assert!(g.event_startup().is_ok());
    assert!(g.event_startup().is_err());
}

#[test]
fn terminal_ephemeral_singleton() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("B", JobKind::Ephemeral);

    g.event_startup().unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.query_ready_for_cleanup().is_empty());
    assert!(g.is_finished());
}

#[test]
fn terminal_ephemeral_24() {
    /*
    A ➔ TB

    So TB does not get run.
    Supposedly
    */
    start_logging();

    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("A", JobKind::Output);
    g.add_node("TB", JobKind::Ephemeral);
    g.depends_on("TB", "A");
    info!("now startup");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["A"]);
    g.event_now_running("A").unwrap();
    g.event_job_finished_success("A", "histA2".to_string())
        .unwrap();
    assert!(g.query_ready_to_run().is_empty());
    assert!(g.query_ready_for_cleanup().is_empty());
    assert!(g.is_finished());
}

fn run_graph(
    mut g: PPGEvaluator<StrategyForTesting>,
    done_log: Rc<RefCell<HashSet<String>>>,
) -> HashMap<String, String> {
    g.event_startup().unwrap();
    while !g.is_finished() {
        for job_id in g.query_ready_to_run().iter() {
            g.event_now_running(job_id).unwrap();
            g.event_job_finished_success(job_id, format!("history_{}", job_id))
                .unwrap();
            done_log.borrow_mut().insert(job_id.clone());
        }
    }
    g.new_history()
}

#[test]
fn test_run_then_add_jobs() {
    let strat = StrategyForTesting::new();
    let init = |history| {
        let strat = strat.clone();
        let mut g = PPGEvaluator::new_with_history(history, strat);
        g.add_node("A", JobKind::Output);
        g
    };
    let g = init(HashMap::new());
    let history = run_graph(g, strat.already_done.clone());

    error!("part2");
    let mut g = init(history);
    g.add_node("B", JobKind::Output);
    g.depends_on("B", "A");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["B"]);
    g.event_now_running("B").unwrap();
    g.event_job_finished_success("B", "history_b".to_string())
        .unwrap();
    assert!(g.is_finished());
    let history = g.new_history();
    dbg!(&history);
    assert_eq!(history.get("A!!!B"), Some(&"history_A".to_string()));
}

#[test]
fn test_issue_20210726a() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("J0", JobKind::Output);
    g.add_node("J2", JobKind::Ephemeral);
    g.add_node("J3", JobKind::Ephemeral);
    g.add_node("J76", JobKind::Output);

    g.depends_on("J0", "J2");
    g.depends_on("J2", "J3");
    g.depends_on("J2", "J76");
    g.depends_on("J76", "J3");
    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J3"]);
    g.event_now_running("J3").unwrap();
    g.event_job_finished_success("J3", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J76"]);
    g.event_now_running("J76").unwrap();
    g.event_job_finished_success("J76", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J2"]);

    g.event_now_running("J2").unwrap();
    g.event_job_finished_success("J2", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J0"]);
    g.event_now_running("J0").unwrap();
    g.event_job_finished_success("J0", "".to_string()).unwrap();
    assert!(g.is_finished())
}
#[test]
fn test_issue_20211001() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("J3", JobKind::Ephemeral);
    g.add_node("J48", JobKind::Ephemeral);
    g.add_node("J61", JobKind::Output);
    g.add_node("J67", JobKind::Always);

    g.depends_on("J61", "J48");
    g.depends_on("J67", "J48");
    g.depends_on("J61", "J3");

    g.event_startup().unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J3", "J48"]);
    g.event_now_running("J3").unwrap();
    g.event_job_finished_success("J3", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J48"]);
    g.event_now_running("J48").unwrap();
    g.event_job_finished_success("J48", "".to_string()).unwrap();
    assert_eq!(g.query_ready_to_run(), set!["J61", "J67"]);
    g.event_now_running("J67").unwrap();
    g.event_job_finished_success("J67", "".to_string()).unwrap();
    g.event_now_running("J61").unwrap();
    g.event_job_finished_success("J61", "".to_string()).unwrap();

    assert!(g.is_finished())
}
#[test]
#[should_panic]
fn test_adding_node_twice() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("J3", JobKind::Ephemeral);
    g.add_node("J3", JobKind::Ephemeral);
}
#[test]
fn test_ephemeral_not_running_without_downstreams() {
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("J3", JobKind::Ephemeral);
    g.event_startup().unwrap();
    assert!(g.is_finished()); // it's not running...

    debug!("part 2");
    let mut g = PPGEvaluator::new(StrategyForTesting::new());
    g.add_node("J3", JobKind::Ephemeral);
    g.add_node("J4", JobKind::Ephemeral);
    g.add_node("J5", JobKind::Ephemeral);
    g.add_node("J6", JobKind::Ephemeral);
    g.add_node("A1", JobKind::Always);
    g.depends_on("J3", "J4");
    g.depends_on("J5", "J6");
    g.depends_on("J6", "J3");
    g.event_startup().unwrap();
    assert!(!g.is_finished());
    g.event_now_running("A1").unwrap();
    g.event_job_finished_failure("A1").unwrap();
    assert!(g.is_finished());
    assert_eq!(g.new_history().len(), 0); //since nothing succeeded
                                          //
                                          //problem: evaluate_next_steps leads to changes
                                          //which will change in the next run, but we would need
                                          //it to run multiple times to uppropagate when ja ephemeral decides *not* to run...
}

#[test]
fn test_simple_graph_runner() {
    let mut ro = TestGraphRunner::new(Box::new(|g| {
        g.add_node("A", JobKind::Output);
    }));
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 1);
    //does not get rerun
    error!("part2");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 1);
    ro.already_done.remove("A");
    error!("part3");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 2);

    ro.setup_graph = Box::new(|g| {
        g.add_node("A", JobKind::Output);
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "A");
    });
    error!("part4");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 2);
    assert_eq!(*ro.run_counters.get("B").unwrap(), 1);

    error!("part5");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 2);
    assert_eq!(*ro.run_counters.get("B").unwrap(), 1);
    ro.already_done.remove("A"); //which then gives a different history

    error!("part6");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 3);
    assert_eq!(*ro.run_counters.get("B").unwrap(), 1); //b has some a->b history, no trigger

    ro.history
        .insert("A!!!B".to_string(), "changedA".to_string());
    ro.already_done.remove("A"); //which then gives a different history
    error!("final part");
    let g = ro.run(&Vec::new());
    assert_eq!(*ro.run_counters.get("A").unwrap(), 4);
    assert_eq!(*ro.run_counters.get("B").unwrap(), 2); // now we trigger
}

#[test]
fn test_nested_too_deply_detection() {
    static MAX_NEST: u32 = 25;
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        let c = MAX_NEST - 1;
        for ii in 0..c {
            g.add_node(&format!("A{}", ii), JobKind::Output);
        }
        for ii in 1..c {
            g.depends_on(&format!("A{}", ii - 1), &format!("A{}", ii));
        }
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    ro.allowed_nesting = MAX_NEST;
    let g = ro.run(&Vec::new()).unwrap();
}

#[test]
fn test_bigish_linear_graph() {
    crate::test_big_linear_graph(100);
    crate::test_big_linear_graph_half_ephemeral(100);
}

#[test]
fn test_ephemeral_one_ephemeral_two_downstreams() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("B", JobKind::Output);
        g.add_node("C", JobKind::Output);
        g.depends_on("B", "TA");
        g.depends_on("C", "TA");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("TA"));
    assert!(new_history.contains_key("B"));
    assert!(new_history.contains_key("C"));
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
}

#[test]
fn test_ephemeral_triangle_just() {
    /* ↓ ←➔ ↑
      *
      *
      TA         TB
       |➔  TC  ← |
           ↓
           D
    */

    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("TC", JobKind::Ephemeral);
        g.add_node("D", JobKind::Output);
        g.depends_on("TC", "TA");
        g.depends_on("TC", "TB");
        g.depends_on("D", "TC");
    }
    start_logging();
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));

    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));
}

#[test]
fn test_ephemeral_triangle_plus() {
    /*
          TA         TB
           |➔  TC  ← |
                ↓
                D ← Ea
    */
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("TC", JobKind::Ephemeral);
        g.add_node("D", JobKind::Output);
        g.add_node("E", JobKind::Always);
        g.depends_on("TC", "TA");
        g.depends_on("TC", "TB");
        g.depends_on("D", "TC");
        g.depends_on("D", "E");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&1));

    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&2));

    ro.outputs
        .insert("E".to_string(), "trigger_inval".to_string());
    let g = ro.run(&Vec::new()).unwrap();

    assert!(ro.run_counters.get("TA") == Some(&2));
    assert!(ro.run_counters.get("TB") == Some(&2));
    assert!(ro.run_counters.get("TC") == Some(&2));
    assert!(ro.run_counters.get("D") == Some(&2));
    assert!(ro.run_counters.get("E") == Some(&3));
}

#[test]
fn test_ephemeral_output_triangle_plus() {
    //same situation as test_ephemeral_triangle_plus, but TC is an now output!
    /*
      TA         TB
       |➔  C  ← |
            ↓
            D ← Ea
    */
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output); // !!!
        g.add_node("D", JobKind::Output);
        g.add_node("E", JobKind::Always);
        g.depends_on("C", "TA");
        g.depends_on("C", "TB");
        g.depends_on("D", "C");
        g.depends_on("D", "E");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&1));

    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
    assert!(ro.run_counters.get("D") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&2));

    ro.outputs
        .insert("E".to_string(), "trigger_inval".to_string());
    let g = ro.run(&Vec::new()).unwrap();

    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1)); // an output does not get rerun..
    assert!(ro.run_counters.get("D") == Some(&2));
    assert!(ro.run_counters.get("E") == Some(&3));
}

#[test]
fn test_ephemeral_downstream_invalidated() {
    /*
       TA ➔  B
       becomes


       TA ➔  B
             ↑
            FI52

    which triggers an invalidation on B
    and a rebuild on TA,
     * */
    start_logging();
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "TA");
    }
    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "TA");

        g.add_node("FI52", JobKind::Always);
        g.depends_on("B", "FI52");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("TA"));
    assert!(new_history.contains_key("B"));
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    error!("Part2");

    ro.setup_graph = Box::new(create_graph2);
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("FI52"));
    dbg!(&ro.run_counters);
    assert!(ro.run_counters.get("FI52") == Some(&1));
    assert!(ro.run_counters.get("TA") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&2));
}

#[test]
fn test_ephemeral_leaf_invalidated() {
    /*
        TA ➔ TB ➔ C
        becomes
        TA ➔ TB ➔ C
                  ↑
                FI52
    */
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "TB");
        g.depends_on("TB", "TA");
    }
    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.add_node("FI52", JobKind::Always);
        g.depends_on("C", "FI52");
        g.depends_on("C", "TB");
        g.depends_on("TB", "TA");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("TA"));
    assert!(new_history.contains_key("TB"));
    assert!(new_history.contains_key("C"));
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    start_logging();
    error!("Part2");

    ro.setup_graph = Box::new(create_graph2);
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("FI52"));
    dbg!(&ro.run_counters);
    assert!(ro.run_counters.get("FI52") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&2));
    assert!(ro.run_counters.get("TA") == Some(&2));
    assert!(ro.run_counters.get("TB") == Some(&2));
}

#[test]

fn test_loosing_an_input_is_invalidating() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Always);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "A");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("C", JobKind::Output);
    }
    error!("part2");
    ro.setup_graph = Box::new(create_graph2);
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("C"));
    assert!(new_history.contains_key("A"));
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&2));
}

#[test]
fn test_changing_inputs_when_leaf_was_missing() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Always);
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "A");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));

    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Output);
        g.add_node("C", JobKind::Always);
        g.depends_on("A", "C")
    }
    ro.setup_graph = Box::new(create_graph2);

    ro.outputs.insert("A".to_string(), "new".to_string());
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("B"));
    assert!(new_history.contains_key("A"));
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    // running it again doesn't run anything but the always job.
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("B"));
    assert!(new_history.contains_key("A"));
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&2));

    fn create_graph3(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Output);
        g.add_node("C", JobKind::Always);
        g.depends_on("A", "C");
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "A");
    }
    error!("part3");
    ro.setup_graph = Box::new(create_graph3);
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&1)); // output is still around, input to B is
                                                   // unchanged, so all good
                                                   // kept the history. A is not being invalidated
    assert!(ro.run_counters.get("C") == Some(&3));
}
#[test]
fn test_replacing_an_input_then_restoring() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Always);
        g.add_node("B", JobKind::Output);
        g.depends_on("B", "A");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));

    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&1));

    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("B", JobKind::Output);
        g.add_node("C", JobKind::Always);
        g.depends_on("B", "C")
    }
    ro.setup_graph = Box::new(create_graph2);

    let g = ro.run(&Vec::new()).unwrap();
    let new_history = g.new_history();
    assert!(new_history.contains_key("C"));
    assert!(new_history.contains_key("B"));
    assert!(new_history.contains_key("A"));
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&2));
    assert!(ro.run_counters.get("C") == Some(&1));

    let g = ro.run(&Vec::new()).unwrap();
    dbg!(&ro.run_counters);
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&2)); // is this
    assert!(ro.run_counters.get("C") == Some(&2));

    ro.setup_graph = Box::new(create_graph); // back to the original one.

    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&3));
    assert!(ro.run_counters.get("B") == Some(&3));
    assert!(ro.run_counters.get("C") == Some(&2));
}

#[test]
fn test_two_ephemerals_one_output_straight() {
    /*
         /* ↓ ← ➔ ↑ */


        A(e)      B(e)
           |        |
           |➔ C(o) ←|

         *

    */
    start_logging();
    //
    // actually A diamend...
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Ephemeral);
        g.add_node("B", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "A");
        g.depends_on("C", "B");
        //g.depends_on("B", "A");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
}

#[test]
fn test_two_ephemerals_one_output_crosslinked() {
    start_logging();
    //
    // actually A diamend...
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Ephemeral);
        g.add_node("B", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "A");
        g.depends_on("C", "B");
        g.depends_on("B", "A");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
}

#[test]
fn test_epheremal_chained_invalidate_intermediate() {
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Ephemeral);
        g.add_node("B", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("B", "A");
        g.depends_on("C", "B");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    //    let new_history = g.new_history();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));
    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("A") == Some(&1));
    assert!(ro.run_counters.get("B") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("A", JobKind::Ephemeral);
        g.add_node("B", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("B", "A");
        g.depends_on("C", "B");

        g.add_node("D", JobKind::Always);
        g.depends_on("B", "D");
    }

    ro.setup_graph = Box::new(create_graph2);

    error!("part 3");

    let g = ro.run(&Vec::new()).unwrap();
    debug!("{:?}", &ro.run_counters);
    assert!(ro.run_counters.get("D") == Some(&1));
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&2));
    assert!(ro.run_counters.get("C") == Some(&2));

    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("D") == Some(&2));
    assert!(ro.run_counters.get("A") == Some(&2));
    assert!(ro.run_counters.get("B") == Some(&2));
    assert!(ro.run_counters.get("C") == Some(&2));
}
#[test]
fn test_ephemeral_tritri() {
    // actually A diamend...
    /*
        /* ↓ ←➔ ↑ */

        TA ➔   TB ➔ TC
        ➔  TD  ←
            ↓
            E
    * */
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("TC", JobKind::Ephemeral);
        g.add_node("TD", JobKind::Ephemeral);
        g.add_node("E", JobKind::Output);
        g.depends_on("TB", "TA");
        g.depends_on("TC", "TB");
        g.depends_on("TD", "TB");
        g.depends_on("TD", "TA");
        g.depends_on("E", "TD");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    dbg!(&ro.run_counters);
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == None); // no downstream, no running
    assert!(ro.run_counters.get("TD") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&1));

    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("TC") == None);
    assert!(ro.run_counters.get("TD") == Some(&1));
    assert!(ro.run_counters.get("E") == Some(&1));
}
#[test]
pub fn test_adding_ephemeral_triggers_rebuild() {
    // actually A diamend...
    fn create_graph(g: &mut PPGEvaluator<StrategyForTesting>) {
        //g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "TB");
    }
    let mut ro = TestGraphRunner::new(Box::new(create_graph));
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TB") == Some(&1));
    assert!(ro.run_counters.get("C") == Some(&1));

    fn create_graph2(g: &mut PPGEvaluator<StrategyForTesting>) {
        g.add_node("TA", JobKind::Ephemeral);
        g.add_node("TB", JobKind::Ephemeral);
        g.add_node("C", JobKind::Output);
        g.depends_on("C", "TB");
        g.depends_on("TB", "TA");
    }

    ro.setup_graph = Box::new(create_graph2);

    error!("part2");
    let g = ro.run(&Vec::new()).unwrap();
    assert!(ro.run_counters.get("TA") == Some(&1));
    assert!(ro.run_counters.get("TB") == Some(&2)); // insulated
    assert!(ro.run_counters.get("C") == Some(&1));
}

#[test]
fn test_ephemeral_retriggered_changing_output() {
    assert!(false);
    // an ephemeral must not change it's output
    // when it get's retriggered (vs invalidated).
    // This should lead to an error,
    // and a failing of all downstreams.
}
