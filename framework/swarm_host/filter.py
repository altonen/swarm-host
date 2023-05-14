import inspect
import textwrap

class NotificationFilter():
    def __init__(self):
        pass

    def export(self):
        return textwrap.dedent(inspect.getsource(self.inject_notification))
