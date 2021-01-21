import os

cpu_count = None


def escape_logging(s):
    return str(s).replace("<", "\\<").replace("{", "{{").replace("}", "}}")


def CPUs():
    """
    Detects the number of CPUs on a system. Cribbed from pp.
    """
    global cpu_count
    if cpu_count is None:
        cpu_count = 1  # default
        # Linux, Unix and MacOS:
        if hasattr(os, "sysconf"):
            if "SC_NPROCESSORS_ONLN" in os.sysconf_names:
                # Linux & Unix:
                ncpus = os.sysconf("SC_NPROCESSORS_ONLN")
                if isinstance(ncpus, int) and ncpus > 0:
                    cpu_count = ncpus
            else:  # OSX: pragma: no cover
                cpu_count = int(
                    os.popen2("sysctl -n hw.ncpu")[1].read()
                )  # pragma: no cover
        # Windows:
        if "NUMBER_OF_PROCESSORS" in os.environ:  # pragma: no cover
            ncpus = int(os.environ["NUMBER_OF_PROCESSORS"])
            if ncpus > 0:
                cpu_count = ncpus
    return cpu_count


def job_or_filename(job_or_filename, invariant_class=None):
    """Take a filename, or a job. Return Path(filename), dependency-for-that-file
    ie. either the job, or a invariant_class (default: FileInvariant)"""
    from .jobs import Job, FileInvariant
    from pathlib import Path

    if invariant_class is None: #pragma: no cover
        invariant_class = FileInvariant

    if isinstance(job_or_filename, Job):
        filename = job_or_filename.files[0]
        deps = [job_or_filename]
    elif job_or_filename is not None:
        filename = Path(job_or_filename)
        deps = [invariant_class(filename)]
    else:
        filename = None
        deps = []
    return filename, deps


def assert_uniqueness_of_object(
    object_with_name_attribute, pipegraph=None, also_check=None
):
    """Makes certain there is only one object with this class & .name.

    This is necessary so the pipegraph jobs assign their data only to the
    objects you're actually working with."""
    if pipegraph is None: #pragma: no branch
        from pypipegraph2 import global_pipegraph

        pipegraph = global_pipegraph

    if object_with_name_attribute.name.find("/") != -1:
        raise ValueError(
            "Names must not contain /, it confuses the directory calculations"
        )
    if not hasattr(pipegraph, "object_uniquifier"):
        pipegraph.object_uniquifier = {}
    typ = object_with_name_attribute.__class__
    if typ not in pipegraph.object_uniquifier:
        pipegraph.object_uniquifier[typ] = {}
    if object_with_name_attribute.name in pipegraph.object_uniquifier[typ]:
        raise ValueError(
            "Doublicate object: %s, %s" % (typ, object_with_name_attribute.name)
        )
    if also_check:
        if not isinstance(also_check, list):
            also_check = [also_check]
        for other_typ in also_check:
            if (
                other_typ in pipegraph.object_uniquifier
                and object_with_name_attribute.name
                in pipegraph.object_uniquifier[other_typ]
            ):
                raise ValueError(
                    "Doublicate object: %s, %s"
                    % (other_typ, object_with_name_attribute.name)
                )
    object_with_name_attribute.unique_id = len(pipegraph.object_uniquifier[typ])
    pipegraph.object_uniquifier[typ][object_with_name_attribute.name] = True


def flatten_jobs(j):
    """Take an arbitrary deeply nested list of lists of jobs
    and return just the jobs"""
    from .jobs import Job

    if isinstance(j, Job):
        yield j
    else:
        for sj in j:
            yield from flatten_jobs(sj)
