def build_result(
    *,
    kind,
    text,
    task,
    backend,
    text_role="source",
    segments=None,
    language="",
    target_language="",
    extra=None,
):
    # Keep "text" as the primary typed value while exposing source/translated fields.
    message = {
        "kind": kind,
        "text": text,
        "segments": segments or [],
        "backend": backend,
        "task": task,
    }

    if language:
        message["language"] = language
    if target_language:
        message["target_language"] = target_language

    if text_role == "translated":
        message["translated_text"] = text
    else:
        message["source_text"] = text

    if extra:
        message.update(extra)

    return message
