#!/usr/bin/env python3
"""A stand-in for DI's stream server: loops an MP3 at real speed, so the app can play with no key. Usage: fake-icecast.py FILE PORT"""
import socket, sys, threading, time

data = open(sys.argv[1], "rb").read()
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", int(sys.argv[2])))
server.listen(5)

def serve(c):
    c.recv(2048)
    c.sendall(b"HTTP/1.0 200 OK\r\nContent-Type: audio/mpeg\r\n\r\n")
    sent, start = 0, time.time()
    try:
        while True:
            want = 32000 + int((time.time() - start) * 16000)
            while sent < want:
                at = sent % len(data)
                piece = data[at:min(len(data), at + want - sent)]
                c.sendall(piece)
                sent += len(piece)
            time.sleep(0.02)
    except OSError:
        pass

while True:
    conn, _ = server.accept()
    threading.Thread(target=serve, args=(conn,), daemon=True).start()
