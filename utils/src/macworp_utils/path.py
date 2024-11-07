"""Utils for dealing with paths."""

from pathlib import Path
import re
from typing import Union


SLASH_SEQ_REGEX: re.Pattern = re.compile(r"\/+")
"""Regex to match multiple sequence of slashes.
"""


def secure_joinpath(base_path: Path, path_to_join: Union[str, Path]) -> Path:
    """
    Joins to paths securely.
    Removes dangerous directory operations from path_to_join, e.g.
    * `/` at path begin would result in a absolut path escaping the base_path
    * `..` can be used to escape the project dir.
    so the path is secure to join with the base_path.

    Parameters
    ----------
    base_path : Path
        Base path to join with.
    path_to_join : Path
        Path to join with the base path.

    Returns
    -------
    Path
        Relative path, secure to join with projects directory.
    """
    if isinstance(path_to_join, str):
        path_to_join = Path(path_to_join)

    parts = list(path_to_join.parts)

    # Remove leading slashes to avoid overwriting the projects directory when using joinpath
    while True:
        if len(parts) == 0 or SLASH_SEQ_REGEX.match(parts[0]) is None:
            break
        parts = parts[1:]

    # Remove all `..` from path as they could be used to escape the base_path
    parts = list(filter(lambda x: x != "..", parts))

    return base_path.joinpath(Path(*parts))


def is_within_path(parent_path: Path, child_path: Path) -> bool:
    """
    Checks if the given path is part of the project's file directory.

    Parameters
    ----------
    parent_path : Path
        Overlaying (absolute) path
    child_path : Path
        Path to check (absolute)

    Returns
    -------
    bool
        True if child_path is parent_path or part of parent_path
    """
    return parent_path == child_path or parent_path in child_path.parents
