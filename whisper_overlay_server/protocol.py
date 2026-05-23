import json
import struct


def send_message(sock, message):
    message_str = json.dumps(message)
    message_bytes = message_str.encode("utf-8")
    message_length = len(message_bytes)
    sock.sendall(struct.pack("!I", message_length))
    sock.sendall(message_bytes)


def recv_exact(sock, length):
    # TCP is a byte stream, so a single recv() call is not guaranteed to fill the frame.
    chunks = []
    remaining = length
    while remaining > 0:
        chunk = sock.recv(remaining)
        if not chunk:
            if remaining == length:
                return None
            raise ConnectionError("connection closed while reading framed message")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def recv_message(sock):
    length_bytes = recv_exact(sock, 4)
    if length_bytes is None:
        return None
    message_length = struct.unpack("!I", length_bytes)[0]
    if message_length & 0x80000000 != 0:
        # Raw audio data
        message_length &= ~0x80000000
        return recv_exact(sock, message_length)

    message_bytes = recv_exact(sock, message_length)
    message_str = message_bytes.decode("utf-8")
    return json.loads(message_str)
