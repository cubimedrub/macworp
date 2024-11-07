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


def make_relative_to(parent_path: Path, child_path: Path) -> Path:
    """
    Makes the given child_path relative to the parent path.

    Parameters
    ----------
    parent_path : Path
        Base path to make the path relative to (absolute).
    child_path : Path
        Path to make relative to the base path (absolute).

    Returns
    -------
    Path
        Path relative to the base path. If the child_path is not within the parent_path, returns `.`.

    Raises
    ------
    ValueError
        If parent_path or child_path are not absolute paths.
    """

    if not parent_path.is_absolute():
        raise ValueError("parent_path must be an absolute path")
    if not child_path.is_absolute():
        raise ValueError("child_path must be an absolute path")
    if not is_within_path(parent_path, child_path):
        return Path(".")

    parent_parts = list(parent_path.parts)
    child_parts = list(child_path.parts)

    if len(parent_parts) > len(child_parts):
        return Path(".")

    relative_path = child_parts[len(parent_parts) :]
    return Path(*relative_path)
