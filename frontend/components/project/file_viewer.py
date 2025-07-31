from pathlib import Path

from nicegui import ui
from typing import List


class FileViewer:
    """Separate component for viewing different file types"""

    def __init__(self, project_id: int):
        self.project_id = project_id

    def show_files(self, file_paths: List[str]):
        """Show all files with viewers"""
        if not file_paths:
            ui.label('No files to display').classes('text-grey-6 text-center')
            return

        with ui.column().classes('w-full gap-2'):
            for filepath in file_paths:
                self._create_file_viewer(filepath)

    def _create_file_viewer(self, filepath: str):
        """Create viewer for filetype"""
        filename = Path(filepath).name

        # Image files
        if self._is_image_file(filepath):
            self._create_image_viewer(filepath, filename)

        # PDF files
        elif filepath.endswith('.pdf'):
            self._create_pdf_viewer(filepath, filename)

        # SVG files
        elif filepath.endswith('.svg'):
            self._create_svg_viewer(filepath, filename)

        # Plotly files
        elif filepath.endswith('.plotly.json'):
            self._create_plotly_viewer(filepath, filename)

        # Table files
        elif self._is_table_file(filepath):
            self._create_table_viewer(filepath, filename)

        # Text files
        elif filepath.endswith('.txt'):
            self._create_text_viewer(filepath, filename)

        else:
            self._create_generic_viewer(filepath, filename)

    def _create_image_viewer(self, filepath: str, filename: str):
        """Create image viewer"""
        with ui.expansion(f'{filename}', icon='image').classes('w-full'):
            with ui.row().classes('w-full justify-center'):
                ui.image(f'/api/projects/{self.project_id}/files/{filepath}').classes('max-w-full max-h-96')

    def _create_pdf_viewer(self, filepath: str, filename: str):
        """Create PDF viewer"""
        with ui.expansion(f'{filename}', icon='picture_as_pdf').classes('w-full'):
            ui.html(f'''
                <embed src="/api/projects/{self.project_id}/files/{filepath}" 
                       type="application/pdf" 
                       width="100%" 
                       height="600px"
                       style="border: 1px solid #ccc;">
            ''')

    def _create_svg_viewer(self, filepath: str, filename: str):
        """Create SVG viewer"""
        with ui.expansion(f'{filename}', icon='code').classes('w-full'):
            if filepath.endswith('.image.svg'):
                # Treat as image
                ui.image(f'/api/projects/{self.project_id}/files/{filepath}').classes('max-w-full max-h-96')
            else:
                # Embed directly in DOM
                ui.html(f'''
                    <div style="text-align: center; border: 1px solid #ccc; padding: 10px;">
                        <object data="/api/projects/{self.project_id}/files/{filepath}" 
                                type="image/svg+xml" 
                                style="max-width: 100%; max-height: 400px;">
                            SVG not supported
                        </object>
                    </div>
                ''')

    def _create_plotly_viewer(self, filepath: str, filename: str):
        """Create Plotly viewer"""
        # Create a safe ID by removing problematic characters
        safe_id = filename.replace('.', '_').replace('-', '_').replace(' ', '_')

        with ui.expansion(f'{filename}', icon='analytics').classes('w-full'):
            ui.add_body_html(f'''
                <div id="plotly-{safe_id}" style="width: 100%; height: 400px;"></div>
                <script>
                    fetch('/api/projects/{self.project_id}/files/{filepath}')
                        .then(response => response.json())
                        .then(data => {{
                            Plotly.newPlot('plotly-{safe_id}', data.data, data.layout);
                        }})
                        .catch(error => console.error('Error loading Plotly data:', error));
                </script>
            ''')

    def _create_table_viewer(self, filepath: str, filename: str):
        """Create table viewer"""
        with ui.expansion(f'{filename}', icon='table_chart').classes('w-full'):
            # This would need to be implemented based on your backend API
            ui.label(f'Table viewer for {filename}').classes('text-center p-4')
            ui.button(
                'Download',
                icon='download',
                on_click=lambda: ui.download(f'/api/projects/{self.project_id}/files/{filepath}')
            )

    def _create_text_viewer(self, filepath: str, filename: str):
        """Create text viewer"""
        # Create a safe ID by removing problematic characters
        safe_id = filename.replace('.', '_').replace('-', '_').replace(' ', '_')

        with ui.expansion(f'{filename}', icon='text_snippet').classes('w-full'):
            ui.add_body_html(f'''
                <div style="background: #f5f5f5; padding: 10px; border-radius: 4px; max-height: 300px; overflow-y: auto;">
                    <pre id="text-content-{safe_id}">Loading...</pre>
                </div>
                <script>
                    fetch('/api/projects/{self.project_id}/files/{filepath}')
                        .then(response => response.text())
                        .then(data => {{
                            document.getElementById('text-content-{safe_id}').textContent = data;
                        }})
                        .catch(error => {{
                            document.getElementById('text-content-{safe_id}').textContent = 'Error loading file: ' + error;
                        }});
                </script>
            ''')

    def _create_generic_viewer(self, filepath: str, filename: str):
        """Create generic file viewer"""
        with ui.expansion(f'{filename}', icon='description').classes('w-full'):
            with ui.row().classes('w-full items-center justify-between'):
                ui.label(f'File: {filename}')
                ui.button(
                    'Download',
                    icon='download',
                    on_click=lambda: ui.download(f'/api/projects/{self.project_id}/files/{filepath}')
                )

    def _is_image_file(self, filepath: str) -> bool:
        """Check if file is an image"""
        image_extensions = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp']
        return any(filepath.lower().endswith(ext) for ext in image_extensions)

    def _is_table_file(self, filepath: str) -> bool:
        """Check if file is a table file"""
        table_extensions = ['.csv', '.tsv', '.xlsx']
        return any(filepath.endswith(ext) for ext in table_extensions)
