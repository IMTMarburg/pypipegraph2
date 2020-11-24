from . import exceptions
import collections
from loguru import logger
import time
import traceback
import networkx
from .util import escape_logging
from .enums import JobKind, ValidationState, JobState
from .jobs import InitialJob


class JobStatus:
    def __init__(self):
        self.state = JobState.Waiting
        self.validation_state = ValidationState.Unknown
        self.input_done_counter = 0
        self.upstreams_completed = False
        self.run_non_invalidated = False
        self.historical_input = {}
        self.historical_output = {}
        self.updated_input = {}
        self.updated_output = {}

        self.start_time = -1
        self.run_time = -1

        self.error = None

    def __str__(self):
        return f"{self.state}, {self.invalidation_state}, 'run_non_invalidated': {self.run_non_invalidated}"


class Runner:
    def __init__(self, job_graph):
        logger.job_trace("Runner.__init__")
        self.jobs = job_graph.jobs.copy()
        self.job_inputs = job_graph.job_inputs.copy()
        self.outputs_to_job_ids = job_graph.outputs_to_job_ids.copy()

        flat_before = networkx.readwrite.json_graph.node_link_data(job_graph.job_dag)
        self.dag = self.extend_dag(job_graph)
        flat_after = networkx.readwrite.json_graph.node_link_data(job_graph.job_dag)
        import json

        assert flat_before == flat_after
        print(
            "dag ",
            json.dumps(
                networkx.readwrite.json_graph.node_link_data(self.dag), indent=2
            ),
        )

        if not networkx.algorithms.is_directed_acyclic_graph(self.dag):
            raise exceptions.NotADag("Extend_dag error")
        self.job_states = {}

        history = job_graph.load_historical()
        for job_id in self.jobs:
            s = JobStatus()
            s.historical_input, s.historical_output = history.get(
                job_id, ({}, {})
            )  # todo: support renaming jobs.
            logger.trace(
                f"Loaded history for {job_id} {len(s.historical_input)}, {len(s.historical_output)}"
            )
            self.job_states[job_id] = s

    def extend_dag(self, job_graph):
        from .jobs import _DownstreamNeedsMeChecker

        dag = job_graph.job_dag.copy()
        known_job_ids = list(networkx.algorithms.dag.topological_sort(dag))
        for job_id in reversed(known_job_ids):
            job = self.jobs[job_id]
            if job.job_kind is JobKind.Temp:
                for downstream_job_id in dag.successors(job_id):
                    # part one: add the 'does the downstream need me to calculate' check?
                    downstream_job = self.jobs[downstream_job_id]
                    if downstream_job.job_kind is not JobKind.Cleanup:
                        downstream_needs_me_checker = _DownstreamNeedsMeChecker(
                            downstream_job
                        )
                        dag.add_node(downstream_needs_me_checker.job_id)
                        self.jobs[
                            downstream_needs_me_checker.job_id
                        ] = downstream_needs_me_checker
                        # self.job_inputs[downstream_needs_me_checker.job_id] =  set() # empty is covered by default duct
                        self.job_inputs[job_id].add(downstream_needs_me_checker.job_id)
                        self.outputs_to_job_ids[
                            downstream_needs_me_checker.job_id
                        ] = downstream_needs_me_checker.outputs[0]

                        dag.add_edge(downstream_needs_me_checker.job_id, job_id)
                        # part two - clone downstreams inputs:
                        # with special attention to temp jobs
                        # to avoid crosslinking
                        for down_upstream_id in self.iter_job_non_temp_upstream_hull(
                            downstream_job_id, dag
                        ):
                            if down_upstream_id != job_id:
                                downstream_upstream_job = self.jobs[down_upstream_id]
                                dag.add_edge(down_upstream_id, job_id)
                                self.job_inputs[job_id].update(
                                    downstream_upstream_job.outputs
                                )
            if hasattr(job, "cleanup_job_class"):
                cleanup_job = job.cleanup_job_class(job)
                self.jobs[cleanup_job.job_id] = cleanup_job
                self.outputs_to_job_ids[cleanup_job.outputs[0]] = cleanup_job.job_id
                for downstream_job_id in dag.neighbors(job_id):
                    dag.add_edge(downstream_job_id, cleanup_job.job_id)
                    self.job_inputs[cleanup_job.job_id].add(downstream_job_id)
        return dag

        # now add an initial job, so we can cut off the evaluation properly

    def iter_job_non_temp_upstream_hull(self, job_id, dag):
        result = []
        for upstream_job_id in dag.predecessors(job_id):
            upstream_job = self.jobs[upstream_job_id]
            if upstream_job.job_kind is JobKind.Temp:
                result.extend(
                    self.iter_job_non_temp_upstream_hull(upstream_job_id, dag)
                )
            else:
                result.append(upstream_job_id)
        return result

    def run(self):
        logger.job_trace("Runner.__run__")

        self.output_hashes = {}
        self.new_history = {}  # what are the job outputs this time.

        job_ids_topological = list(networkx.algorithms.dag.topological_sort(self.dag))

        def is_initial(job_id):
            return (
                not self.job_inputs[job_id]
                and not self.jobs[job_id].is_temp_job()
                and self.jobs[job_id].output_needed(self)
            )

        initial_job_ids = [x for x in job_ids_topological if is_initial(x)]
        self.open_job_ids = [x for x in job_ids_topological if not is_initial(x)]
        self.events = collections.deque()
        for job_id in initial_job_ids:
            self.job_states[job_id].state = JobState.ReadyToRun
            self.push_event("JobReady", (job_id,))
        while self.events:
            ev = self.events.popleft()
            logger.job_trace(f"<-handle {ev[0]} {escape_logging(ev[1][0])}")
            if ev[0] == "JobSuccess":
                self.handle_job_success(*ev[1])
            elif ev[0] == "JobSkipped":
                self.handle_job_skipped(*ev[1])
            elif ev[0] == "JobReady":
                self.handle_job_ready(*ev[1])
            elif ev[0] == "JobFailed":
                self.handle_job_failed(*ev[1])
            else:
                raise NotImplementedError(ev[0])
        logger.job_trace("Left runner.run()")
        return self.job_states

    def handle_job_success(self, job_id, job_outputs):
        job = self.jobs[job_id]
        job_state = self.job_states[job_id]
        # record our success
        logger.job_trace(f"\t{escape_logging(str(job_outputs)[:50])}...")
        for name, hash in job_outputs.items():
            if name not in job.outputs:
                job_state.error = exceptions.JobContractError(
                    f"\t{job_id} returned undeclared output {name}"
                )
                logger.warning(job_state.error)
                self.fail_downstream(job.outputs, job_id)
                job_state.status = JobStatus.Failed
                break
            logger.job_trace(f"\tCapturing hash for {name}")
            self.output_hashes[name] = hash
            job_state.updated_output[name] = hash
            # when the job is done, it's the time time to record the inputs
            # job_state.updated_input = {
            # name: self.output_hashes[name]
            # for name in self.get_job_inputs(job.job_id)
            # }
            job_state.state = JobState.Executed

        self.inform_downstreams_of_outputs(job_id, job_outputs)

    def inform_downstreams_of_outputs(self, job_id, job_outputs):
        job = self.jobs[job_id]

        for downstream_id in self.dag.successors(job_id):
            logger.job_trace(f"\t\tDownstream {downstream_id}")
            downstream_state = self.job_states[downstream_id]
            downstream_job = self.jobs[downstream_id]
            for name, hash in job_outputs.items():
                if name in self.job_inputs[downstream_id]:
                    logger.job_trace(f"\t\t\tHad {name}")
                    old = downstream_state.historical_input.get(name, None)
                    new = hash
                    if new != "IgnorePlease" and (
                        new == "ExplodePlease"
                        or not self.compare_history(old, new, job.__class__)
                    ):
                        logger.job_trace("\t\t\tinput changed -> invalidate")
                        downstream_state.validation_state = ValidationState.Invalidated
                    downstream_state.updated_input[name] = hash  # update any way.
                else:
                    logger.job_trace(f"\t\t\tNot an input {name}")
            if self.all_inputs_finished(downstream_id):
                if (
                    downstream_job.job_kind is JobKind.Temp
                    and downstream_state.validation_state is ValidationState.Invalidated
                ):
                    logger.job_trace(f"{downstream_id} was Temp")
                    if self.job_has_non_temp_somewhere_downstream(downstream_id):
                        self.push_event("JobReady", (downstream_id,), 3)
                    else:
                        self.push_event("JobSkipped", (downstream_id,), 3)
                elif (
                    downstream_state.validation_state is ValidationState.Invalidated
                    or downstream_job.output_needed(self)
                ):
                    self.push_event("JobReady", (downstream_id,), 3)
                else:
                    if len(downstream_state.updated_input) < len(
                        downstream_state.historical_input
                    ):
                        logger.job_trace(f"\t\t\thistorical_input {downstream_state.historical_input.keys()}")
                        logger.job_trace("\t\t\tinput disappeared -> invalidate")
                        downstream_state.validation_state = ValidationState.Invalidated
                        self.push_event("JobReady", (downstream_id,), 3)
                    else:
                        downstream_state.validation_state = ValidationState.Validated
                        self.push_event("JobSkipped", (downstream_id,), 3)

    def handle_job_skipped(self, job_id):
        job_state = self.job_states[job_id]
        job_state.state = JobState.Skipped
        job_state.updated_output = job_state.historical_output.copy()
        # the input has already been filled.
        self.inform_downstreams_of_outputs(
            job_id, job_state.updated_output
        )  # todo: leave off for optimization - should not trigger anyway.

    def handle_job_ready(self, job_id):
        job = self.jobs[job_id]
        job_state = self.job_states[job_id]
        try:
            logger.job_trace(f"\tExecuting {job_id}")
            job.start_time = time.time()
            outputs = job.run(self, job_state.historical_output)
            job.run_time = time.time() - job.start_time
            self.push_event("JobSuccess", (job_id, outputs))
        except Exception as e:
            job_state.error = str(e) + "\n" + traceback.format_exc()
            logger.warning(f"Execute {job_id} failed: {escape_logging(e)}")
            self.push_event("JobFailed", (job_id, job_id))

    def handle_job_failed(self, job_id, source):
        job = self.jobs[job_id]
        job_state = self.job_states[job_id]
        job_state.state = JobState.Failed
        self.fail_downstream(job.outputs, job_id)

    def all_inputs_finished(self, job_id):
        job_state = self.job_states[job_id]
        if job_state.state in (JobState.Failed, JobState.UpstreamFailed):
            return False
        logger.job_trace(f"\t\t\tjob_inputs: {escape_logging(self.job_inputs[job_id])}")
        logger.job_trace(
            f"\t\t\tupdated_input: {escape_logging(self.job_states[job_id].updated_input.keys())}"
        )

        return len(self.job_states[job_id].updated_input) == len(
            self.job_inputs[job_id]
        )

    def push_event(self, event, args, indent=0):
        logger.opt(depth=1).log("JobTrace", "\t" * indent + f"->push {event} {args[0]}")
        self.events.append((event, args))

    def fail_downstream(self, outputs, source):
        logger.job_trace(f"failed_downstream {outputs} {source}")
        for output in outputs:
            # can't I run this with the job_id? todo: optimization
            job_id = self.outputs_to_job_ids[
                output
            ]  # todo: don't continue if the state is already failed...
            for node in self.dag.successors(job_id):
                self.job_states[node].state = JobState.UpstreamFailed
                self.job_states[node].error = f"Upstream {source} failed"

    def compare_history(self, old_hash, new_hash, job_class):
        if old_hash is None:
            return False
        return job_class.compare_hashes(old_hash, new_hash)

        if old_hash == new_hash:
            return True
        # FileInvariant - ignore
        if (
            "hash" in new_hash
            and "hash" in old_hash
            and new_hash["hash"] == old_hash["hash"]
        ):
            return True
        # logger.trace(
        # f"Comparing {old_hash} and {new_hash}".replace("{", "{{").replace("}", "}}")
        # )
        return (
            False  # todo: this needs expanding...depending on what kind of hash it is.
        )

    def get_job_inputs(self, job_id):
        return self.job_inputs[job_id]
        # return networkx.algorithms.dag.ancestors(self.job_graph.job_dag, job.job_id)

    def job_has_non_temp_somewhere_downstream(self, job_id):
        for downstream_id in self.dag.neighbors(job_id):
            j = self.jobs[downstream_id]
            if j.job_kind is not JobKind.Temp:
                return True
            else:
                if self.job_has_non_temp_somewhere_downstream(downstream_id):
                    return True
        return False
