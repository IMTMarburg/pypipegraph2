#!/usr/bin/env python3

"""This creates one FileGeneratingJob with a few hundred of direct DataLoadingJobs,
which exposed a o(n**3) or so bottleneck in runner.modify_dag
"""
import pypipegraph2 as ppg
import shutil
from pathlib import Path
import os
import sys

run_dir = (Path(__file__).parent / "run").absolute()

try:
    count = int(sys.argv[1])
except:
    count = 200


def simple():
    if run_dir.exists():
        shutil.rmtree(run_dir)
    run_dir.mkdir()
    os.chdir(run_dir)
    ppg.new()

    data = {}

    def final(of):
        of.write_text(str(len(data)))

    final_job = ppg.FileGeneratingJob("final", final)

    last = final_job
    for ii in range(count):

        def load(ii=ii):
            data[ii] = str(ii)

        loader = ppg.DataLoadingJob(f"dl{ii}", load)
        last.depends_on(loader)
        last = loader
    ppg.run()


def test_simple(benchmark):
    print(run_dir)
    benchmark(simple)


if __name__ == "__main__":
    simple()
