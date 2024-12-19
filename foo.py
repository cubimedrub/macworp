from http.server import HTTPServer, BaseHTTPRequestHandler
import uuid


class SimpleHTTPRequestHandler(BaseHTTPRequestHandler):

    def do_PUT(self):
        print("\n\n\nPUT", self.path)

        content_length = int(self.headers["Content-Length"])
        body = self.rfile.read(content_length)
        print(body.decode())
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"")

    def do_POST(self):
        print("\n\n\nPOST", self.path)
        print(self.headers)

        content_length = int(self.headers["Content-Length"])
        body = self.rfile.read(content_length)
        print(body.decode())
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"")

    def do_GET(self):
        print("\n\n\nGET", self.path)
        path_only = self.path.split("?")[0]
        match path_only:
            case "/snakemake/api/service-info":
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"status": "running"}')
            case "/snakemake/create_workflow":
                self.send_response(200)
                self.end_headers()
                self.wfile.write(f'{{"id": "{str(uuid.uuid4())}"}}'.encode("utf-8"))
            case _:
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"")


httpd = HTTPServer(("localhost", 9000), SimpleHTTPRequestHandler)
httpd.serve_forever()
