import sys

sys.path.append("../../python")
import pypipegraph2 as ppg
from pathlib import Path
import shutil


whitelist = ["simplest.py"]

for fn in Path(".").glob("*"):
    if not fn.name in whitelist:
        if fn.is_dir():
            shutil.rmtree(fn)
        else:
            fn.unlink()


ppg.new(log_level=1, cores=2)
ppg.util.do_jobtrace_log = True


class Dummy(object):
    pass


def write(filename, text):
    Path(filename).write_text(text)


def append(filename, text):
    p = Path(filename)
    if p.exists():
        old = p.read_text()
    else:
        old = ""
    p.write_text(old + text)


def writeappend(filename_write, filename_append, string):
    write(filename_write, string)
    append(filename_append, string)


def read(filename):
    return Path(filename).read_text()


def counter(filename):
    """Helper for counting invocations in a side-effect file"""
    try:
        res = int(Path(filename).read_text())
    except:  # noqa: E722
        res = 0
    Path(filename).write_text(str(res + 1))
    return str(res)


def force_load(job):
    """Force the loading of a Dataloading job that has no other dependents"""
    import pypipegraph2 as ppg

    ppg.JobGeneratingJob(job.job_id + "_gen", lambda: None).depends_on(job)


def simplest():
    job = ppg.FileGeneratingJob("deleteme", lambda of: of.write_text("hello"))


def cached_dl():
    Path('out').mkdir(exist_ok=True)
    o = Dummy()

    def calc():
        return ", ".join(str(x) for x in range(0, 100))

    def store(value):
        o.a = value

    job, cache_job = ppg.CachedDataLoadingJob("out/mycalc", calc, store)
    of = "out/A"

    def do_write(of):
        write(of, o.a)

    ppg.FileGeneratingJob(of, do_write).depends_on(job)
    ppg.run()
    assert read(of) == ", ".join(str(x) for x in range(0, 100))


cached_dl()

ppg.run()
