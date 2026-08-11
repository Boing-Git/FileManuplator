import mimetypes
from pathlib import Path

def detect_mime_type(path: Path) -> str:
    try:
        import magic
        try:
            # Try the standard PyPI 'python-magic' API (Your machine)
            detector = magic.Magic(mime=True)
            return detector.from_file(str(path))
        except TypeError:
            # Fallback for the system 'python3-magic' API (Dad's machine)
            m = magic.open(magic.MAGIC_MIME_TYPE)
            m.load()
            result = m.file(str(path))
            m.close()
            return result
    except (ImportError, AttributeError, Exception):
        pass
    
    # Ultimate fallback if libmagic is missing or completely broken
    mime, _ = mimetypes.guess_type(path)
    return mime or "application/octet-stream"
