import json
import os
from pathlib import Path
from typing import List, Dict

import pandas as pd
from nicegui import ui


class FileViewer:
    """Separate component for viewing different file types with metadata support"""

    def __init__(self, project_id: int):
        self.project_id = project_id

    def show_files(self, file_paths: List[str]):
        """Show all files with viewers and automatically load metadata from .mmdata files

        Args:
            file_paths: List of file paths to display
        """
        if not file_paths:
            ui.label('No files to display').classes('text-grey-6 text-center')
            return

        with ui.column().classes('w-full gap-2'):
            for filepath in file_paths:
                file_metadata = self._load_metadata(filepath)
                self._create_file_viewer(filepath, file_metadata)

    def _load_metadata(self, filepath: str) -> Dict:
        """Load metadata from the corresponding.mmdata file

        Args:
            filepath: Path to the main file

        Returns:
            Dictionary with 'description' and 'header' keys, empty if no metadata file exists
        """
        # Construct path to metadata file
        file_path = Path(filepath)
        metadata_filename = f"{file_path}.mmdata"
        metadata_path = os.path.join('projects', str(self.project_id), 'results', metadata_filename)
        try:
            if os.path.exists(metadata_path):
                with open(metadata_path, 'r', encoding='utf-8') as f:
                    metadata = json.load(f)
                    return {
                        'description': metadata.get('description', ''),
                        'header': metadata.get('header', '')
                    }
        except Exception as e:
            print(f"Error loading metadata for {filepath}: {e}")

        return {'description': '', 'header': ''}

    def _create_file_viewer(self, filepath: str, metadata: Dict):
        """Create viewer for filetype"""
        filename = Path(filepath).name
        description = metadata.get('description', '')
        header = metadata.get('header', '')

        # Build title with metadata
        title = f"{filename} - {header}" if header else filename

        # Image files
        if self._is_image_file(filepath):
            self._create_image_viewer(filepath, title, description)

        # PDF files
        elif filepath.endswith('.pdf'):
            self._create_pdf_viewer(filepath, title, description)

        # SVG files
        elif filepath.endswith('.svg'):
            self._create_svg_viewer(filepath, title, description)

        # Plotly files
        elif filepath.endswith('.plotly.json'):
            self._create_plotly_viewer(filepath, title, description)

        # Table files
        elif self._is_table_file(filepath):
            self._create_table_viewer(filepath, title, description)

        # Text files
        elif filepath.endswith('.txt'):
            self._create_text_viewer(filepath, title, description)

        #else:
           #self._create_generic_viewer(filepath, title, description)

    def _add_metadata_info(self, description: str):
        """Add metadata description if available"""
        if description and description.strip():  # Check for non-empty description
            with ui.card().classes('w-full mb-2 bg-blue-50 border-l-4 border-blue-400'):
                with ui.card_section().classes('py-2 px-3'):
                    with ui.row().classes('items-start gap-2'):
                        ui.icon('info').classes('text-blue-600 mt-1')
                        ui.label(description).classes('text-sm text-blue-800 flex-1')


    def _create_image_viewer(self, filepath: str, title: str, description: str):
        """Create image viewer"""
        with ui.expansion(title, icon='image').classes('w-full'):
            self._add_metadata_info(description)
            with ui.row().classes('w-full justify-center'):
                ui.image(f'projects/{self.project_id}/results/{filepath}').classes('max-w-full max-h-96')

    def _create_pdf_viewer(self, filepath: str, title: str, description: str):
        """Create PDF viewer"""
        with ui.expansion(title, icon='picture_as_pdf').classes('w-full'):
            self._add_metadata_info(description)
            ui.html(f'''
                <embed src="{self.project_id}/results/{filepath}" 
                       type="application/pdf" 
                       width="100%" 
                       height="600px"
                       style="border: 1px solid #ccc;">
            ''')

    def _create_svg_viewer(self, filepath: str, title: str, description: str):
        """Create SVG viewer"""
        with ui.expansion(title, icon='code').classes('w-full'):
            self._add_metadata_info(description)
            if filepath.endswith('.image.svg'):
                ui.image(f'projects/{self.project_id}/results/{filepath}').classes('max-w-full max-h-96')
            else:
                ui.html(f'''
                    <div style="text-align: center; border: 1px solid #ccc; padding: 10px;">
                        <object data="{self.project_id}/results/{filepath}" 
                                type="image/svg+xml" 
                                style="max-width: 100%; max-height: 400px;">
                            SVG not supported
                        </object>
                    </div>
                ''')

    def _create_plotly_viewer(self, filepath: str, title: str, description: str):
        """Create Plotly viewer"""
        with ui.expansion(title, icon='analytics').classes('w-full'):
            self._add_metadata_info(description)
            try:
                full_path = os.path.join('projects', str(self.project_id), 'results', filepath)
                with open(full_path, 'r') as f:
                    plotly_json = json.load(f)

                ui.plotly(plotly_json)
            except Exception as e:
                ui.label(f'Fehler beim Laden der Plotly-Daten: {e}').classes('text-red-500')

    def _create_table_viewer(self, filepath: str, title: str, description: str):
        """Create table"""
        with ui.expansion(title, icon='table_chart').classes('w-full'):
            self._add_metadata_info(description)

            full_path = os.path.join('projects', str(self.project_id), 'results', filepath)

            try:
                # Dateityp erkennen
                if filepath.endswith('.csv'):
                    df = pd.read_csv(full_path)
                elif filepath.endswith('.tsv'):
                    df = pd.read_csv(full_path, sep='\t')
                elif filepath.endswith('.xlsx'):
                    df = pd.read_excel(full_path)
                else:
                    ui.label('Unsupported file type').classes('text-red-500')
                    return

                # Tabelle anzeigen
                ui.table(
                    columns=[{'name': col, 'label': col, 'field': col} for col in df.columns],
                    rows=df.to_dict(orient='records'),
                    row_key='index',
                ).classes('w-full')

            except Exception as e:
                ui.label(f'Error reading file: {e}').classes('text-red-500')

            ui.button(
                'Download',
                icon='download',
                on_click=lambda: ui.download(f'projects/{self.project_id}/results/{filepath}')
            )

    def _create_text_viewer(self, filepath: str, title: str, description: str):
        """Create text viewer"""
        with ui.expansion(title, icon='text_snippet').classes('w-full'):
            self._add_metadata_info(description)
            try:
                full_path = f'projects/{self.project_id}/results/{filepath}'
                with open(full_path, 'r', encoding='utf-8') as f:
                    content = f.read()
            except Exception as e:
                content = f'Fehler beim Laden der Datei: {e}'

            ui.html(f'''
                <div style="background: #000000; color: white; padding: 10px; border-radius: 4px; max-height: 300px; overflow-y: auto; white-space: pre-wrap;">
                    {content}
                </div>
            ''')

    def _create_generic_viewer(self, filepath: str, title: str, description: str):
        """Create generic file viewer"""
        with ui.expansion(title, icon='description').classes('w-full'):
            self._add_metadata_info(description)
            with ui.row().classes('w-full items-center justify-between'):
                ui.label(f'File: {Path(filepath).name}')
                ui.button(
                    'Download',
                    icon='download',
                    on_click=lambda: ui.download(f'projects/{self.project_id}/results/{filepath}')
                )

    def _is_image_file(self, filepath: str) -> bool:
        """Check if file is an image"""
        image_extensions = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp']
        return any(filepath.lower().endswith(ext) for ext in image_extensions)

    def _is_table_file(self, filepath: str) -> bool:
        """Check if file is a table file"""
        table_extensions = ['.csv', '.tsv', '.xlsx']
        return any(filepath.endswith(ext) for ext in table_extensions)
