from .realtime_stt import RealtimeSttEngine


def create_engine(name, args, logger, publish_result):
    if name == "realtime-stt":
        return RealtimeSttEngine(args, logger, publish_result)

    raise ValueError(f"unsupported backend: {name}")
