from os.path import dirname, basename, isfile, join
import glob

modules = glob.glob(join(dirname(__file__), "*_controller.py"))
__all__ = [ basename(f)[:-3] for f in modules if isfile(f)]

from .api import *