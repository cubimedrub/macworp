import pandas as pd
from nicegui import ui




def table_view(self):
    #todo aus service lesen und anzeigen
    df = pd.DataFrame(data={'col1': [1, 2], 'col2': [3, 4]})
    ui.table.from_pandas(df).classes('max-h-40')