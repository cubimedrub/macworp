def add_allow_cors_headers(response):
    """
    Adds CORS headers
    """
    response.headers['Access-Control-Allow-Origin'] = '*'
    response.headers['Access-Control-Allow-Headers'] = 'Content-Type'
    return response