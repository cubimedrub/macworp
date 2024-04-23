import logging


LOGGER_BASE_NAME: str = "nf_cloud_worker"

def get_logger(subname: str, log_level: int) -> logging.Logger:
    """
    Create logger with the given name and log_level

    Parameters
    ----------
    subname : str
        Subname for logger
    log_level : int
        Log level as defined in logging module

    Returns
    -------
    logging.Logger
    """
    logging.basicConfig(handlers=[])
    logger = logging.getLogger(LOGGER_BASE_NAME)
    logger.setLevel(log_level)
    handler = logging.StreamHandler()
    handler.setFormatter(logging.Formatter(f"[%(levelname)s] %(name)s[{subname}] - %(asctime)s - %(message)s"))
    handler.setLevel(log_level)
    logger.addHandler(handler)


    # Silence pika logger
    logging.getLogger("pika").setLevel(logging.CRITICAL)

    return logger

def verbosity_to_log_level(verbosity: int) -> int:
    """
    Convert verbosity level to log level

    Parameters
    ----------
    verbosity : int
        Verbosity level

    Returns
    -------
    int
        Log level as defined in logging module
    """
    match verbosity:
        case 0:
            return logging.CRITICAL
        case 1:
            return logging.ERROR
        case 2:
            return logging.WARNING
        case 3:
            return logging.INFO
        case _:
            return logging.DEBUG
