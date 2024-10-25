"""Errors regarding unknown table formats"""

# std imports
from typing import List


class UnknownTableFormat(Exception):
    """Error raised when table format is unsupported. As hint the supported table formats are provided."""

    def __init__(self, supported_table_formats: List[str]):
        """Create new instance of UnknownTableFormat.

        Parameters
        ----------
        supported_table_formats : List[str]
            Otherwise supported table formats
        """
        self.supported_table_formats = supported_table_formats
