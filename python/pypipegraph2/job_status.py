"""
# todo: rename file.


"""


from multiprocessing.sharedctypes import Value
from .enums import (
    JobOutcome,
)

class RecordedJobOutcome:
    """Job run information collector"""

    def __init__(
            self, 
            job_id,
            outcome,
            payload
            ):
        if not  isinstance(outcome, JobOutcome):
            raise ValueError("Not an JobOutcome")
        self.job_id = job_id
        self.outcome = outcome
        self.payload = payload
        self.runtime = -1


    @property
    def error(self):
        if self.outcome is JobOutcome.Failed:
            return self.payload
        else:
            raise AttributeError("No error on on non failed JobOutcomes")