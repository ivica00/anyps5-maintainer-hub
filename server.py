import http.server
import socketserver
import webbrowser
import os

PORT = 8000
DIRECTORY = os.path.dirname(os.path.abspath(__file__))

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        from urllib.parse import urlsplit, unquote
        allowed = {"/", "/index.html", "/app.js", "/prs_data.js", "/favicon.ico", "/assets/app.css", "/assets/supabase.js", "/assets/LICENSE.supabase"}
        if unquote(urlsplit(self.path).path) not in allowed:
            self.send_error(404)
            return
        super().do_GET()

    def do_HEAD(self):
        from urllib.parse import urlsplit, unquote
        if unquote(urlsplit(self.path).path) not in {"/", "/index.html", "/app.js", "/prs_data.js", "/favicon.ico", "/assets/app.css", "/assets/supabase.js", "/assets/LICENSE.supabase"}:
            self.send_error(404)
            return
        super().do_HEAD()

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)

def run():
    with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
        url = f"http://localhost:{PORT}/index.html"
        print(f"======================================================")
        print(f" AnyPS5 Maintainer Hub running at: {url}")
        print(f" Press Ctrl+C to stop the server.")
        print(f"======================================================")
        webbrowser.open(url)
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\nShutting down server.")

if __name__ == "__main__":
    run()
