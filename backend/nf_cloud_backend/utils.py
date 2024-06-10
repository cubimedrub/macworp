from datetime import datetime

def create_timestamp(date):
    return_data = datetime(year = date.year, month = date.month, day = date.day)
    return return_data.timestamp()