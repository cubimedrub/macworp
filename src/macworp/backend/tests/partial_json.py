"""
This module defines the declarative specification format used to determine if the result JSON from an API call is accepted.
"""

from typing import Any


class DontCare:
    def __init__(self):
        pass

    def __str__(self):
        return "..."

DONT_CARE = DontCare()
"""
Special object that matches any JSON value.
"""

PartialJson = None | bool | int | float | str | list["PartialJson"] | dict[str, "PartialJson"] | DontCare
"""
Can be a JSON value or the special value DONT_CARE, which signifies that any value present is fine.
"""

def match_partial_json(actual: Any, expected: PartialJson) -> bool:
    """
    Returns true if actual is a JSON value that matches the specification given by expected.
    """

    if isinstance(expected, DontCare):
        return True    
    if isinstance(actual, list) and isinstance(expected, list):
        if len(actual) != len(expected):
            return False
        for i in range(len(actual)):
            if not match_partial_json(actual[i], expected[i]):
                return False
        return True
    if isinstance(actual, dict) and isinstance(expected, dict):
        if actual.keys() != expected.keys():
            return False
        for key in actual.keys():
            if not match_partial_json(actual[key], expected[key]):
                return False
        return True
    return actual == expected

