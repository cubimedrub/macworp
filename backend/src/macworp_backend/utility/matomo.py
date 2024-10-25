import traceback

from flask import Flask
from piwikapi.tracking import PiwikTracker
from piwikapi.tests.request import FakeRequest

from macworp_backend.utility.headers.accept_language import AcceptLanguage


def track_request(
    user_agent: str,
    client_ip: str,
    referer: str,
    accept_language_header: str,
    server_name: str,
    url_path: str,
    query_string: str,
    is_https: bool,
    matomo_url: str,
    matomo_site_id: int,
    matomo_auth_token: str,
    app: Flask,
    is_debug: bool,
):
    try:
        accept_languages = AcceptLanguage.parse(accept_language_header)
        accept_language_code = ""
        if len(accept_languages):
            accept_language_code = accept_languages[0].language_code
        matomo_request = FakeRequest(
            {
                "HTTP_USER_AGENT": user_agent,
                "REMOTE_ADDR": client_ip,
                "HTTP_REFERER": referer,
                "HTTP_ACCEPT_LANGUAGE": accept_language_code,
                "SERVER_NAME": server_name,
                "PATH_INFO": url_path,
                "QUERY_STRING": query_string,
                "HTTPS": is_https,
            }
        )
        piwiktracker = PiwikTracker(matomo_site_id, matomo_request)
        piwiktracker.set_api_url(f"{matomo_url}/matomo.php")
        piwiktracker.set_ip(client_ip)  # Optional, to override the IP
        piwiktracker.set_token_auth(matomo_auth_token)  # Optional, to override the IP
        piwiktracker.do_track_page_view("API")
    except:
        if is_debug:
            app.logger.error(traceback.format_exc())  # pylint: disable=no-member
