#!/usr/bin/env python3

"""
A server for RealtimeSTT made to be used with whisper-overlay.
"""

import argparse
import json
import logging
import queue
import socket
import threading

from whisper_overlay_server.engines import create_engine
from whisper_overlay_server.protocol import recv_message, send_message

class Client:
    def __init__(self, tag, conn):
        self.tag = tag
        self.conn = conn
        self.thread = threading.current_thread()
        self.mode = None
        self.is_true_client = False
        self.waiting = False
        self.queue = queue.Queue()

clients = {}
active_client = None
model_lock = threading.Lock()
engine = None

def publish(obj, client=None):
    msg = json.dumps(obj)
    if client is None:
        for c in clients.values():
            if c.mode == "status":
                c.queue.put(msg)
    else:
        client.queue.put(msg)

def refresh_status(client=None):
    publish(dict(refresh_status=True), client=client)

def publish_active_result(message):
    if active_client is not None:
        active_client.queue.put(message)

def handle_client(conn, addr):
    global active_client
    tag = f"{addr[0]}:{addr[1]}"
    client = Client(tag, conn)
    clients[addr] = client

    try:
        logger.info(f'{tag} Connected to client')
        init = recv_message(conn)
        logger.info(f'{tag} Client requested mode {init["mode"]}')
        client.mode = init["mode"]
        client.is_true_client = init["mode"] == "stream"

        if init["mode"] == "status":
            refresh_status(client) # refresh once after startup

            while True:
                message = json.loads(client.queue.get())

                if "refresh_status" in message and message["refresh_status"] == True:
                    n_clients = len(list(filter(lambda x: x.is_true_client, clients.values())))
                    n_waiting = len(list(filter(lambda x: x.is_true_client and x.waiting, clients.values())))
                    status = {
                        "clients": n_clients,
                        "waiting": n_waiting,
                    }
                    send_message(conn, status)
                    client.queue.task_done()
        else:
            logger.info(f'{tag} Acquiring lock')
            client.waiting = True
            refresh_status()
            send_message(conn, dict(status="waiting for lock"))

            with model_lock:
                active_client = client
                client.waiting = False
                refresh_status()
                send_message(conn, dict(status="lock acquired"))
                engine.start()

                def send_queue():
                    try:
                        while True:
                            message = client.queue.get()
                            if message is None:
                                return
                            send_message(conn, message)
                            client.queue.task_done()
                    except (OSError, ConnectionError):
                        logger.info(f"{tag} error in send queue: connection closed?")

                sender_thread = threading.Thread(target=send_queue)
                sender_thread.daemon = True
                sender_thread.start()

                try:
                    while True:
                        msg = recv_message(conn)
                        if msg is None:
                            break

                        if isinstance(msg, bytes):
                            engine.feed_audio(msg)
                            continue

                        if "action" in msg and msg["action"] == "flush":
                            engine.flush()
                            continue
                        else:
                            logger.info(f"{tag} error in recv: invalid message: {msg}")
                            continue
                except (OSError, ConnectionError):
                    logger.info(f"{tag} error in recv: connection closed?")
                finally:
                    client.queue.put(None)
                    active_client = None
                    engine.stop()
                    sender_thread.join()
    except Exception as e:
        import traceback
        traceback.print_exc()
        logger.error(f'{tag} Error handling client: {e}')
    finally:
        refresh_status()
        del clients[addr]
        conn.close()
        logger.info(f'{tag} Connection closed')

if __name__ == "__main__":
    logging.basicConfig(format="%(levelname)s %(message)s")
    logger = logging.getLogger("realtime-stt-server")
    logger.setLevel(logging.INFO)

    parser = argparse.ArgumentParser()
    parser.add_argument("--host", type=str, default='localhost',
        help="The host to listen on [default: 'localhost']")
    parser.add_argument("--port", type=int, default=7007,
        help="The port to listen on [default: 7007]")
    parser.add_argument("--backend", type=str, default="realtime-stt", choices=["realtime-stt"],
        help="The transcription backend to use [default: 'realtime-stt']")
    parser.add_argument("--device", type=str, default="cuda",
        help="Device to run the models on, defaults to cuda if available, else cpu [default: 'cuda']")
    parser.add_argument("--model", type=str, default="large-v3",
        help="Main model used to generate the final transcription [default: 'large-v3']")
    parser.add_argument("--model-realtime", type=str, default="base",
        help="Faster model used to generate live transcriptions [default: 'base']")
    parser.add_argument("--language", type=str, default="",
        help="Set the spoken language. Leave empty to auto-detect. [default: '']")
    parser.add_argument("--debug", action="store_true",
        help="Enable debug log output [default: unset]")

    args = parser.parse_args()
    if args.debug:
        logger.setLevel(logging.DEBUG)
        logging.getLogger().setLevel(logging.DEBUG)

    engine = create_engine(args.backend, args, logger, publish_active_result)
    engine.initialize()

    logger.info(f'Starting server on {args.host}:{args.port}')
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind((args.host, args.port))
        s.listen()
        logger.info(f'Server ready to accept connections')

        try:
            while True:
                # Accept incoming connection
                conn, addr = s.accept()
                conn.setblocking(True)

                # Create a new thread to handle the client
                client_thread = threading.Thread(target=handle_client, args=(conn, addr))
                client_thread.daemon = True # die with main thread
                client_thread.start()

                # Note: The main thread continues to accept new connections
        except KeyboardInterrupt:
            logger.info(f'Received shutdown request')

        for c in clients.values():
            try:
                c.conn.close()
            except (OSError, ConnectionError):
                pass
        try:
            s.close()
        except (OSError, ConnectionError):
            pass

        engine.shutdown()

    logger.info('Server terminated')
