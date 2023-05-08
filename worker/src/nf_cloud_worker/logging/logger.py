import logging

class Logger:
    """
    Logger for message logging
    """

    @classmethod
    def get_logger(cls, name: str, log_level: int) -> logging.Logger:
        """
        Create logger with the given name and log_level

        Parameters
        ----------
        name : str
            Logger name
        log_level : int
            Log level as defined in logging module

        Returns
        -------
        logging.Logger
        """
        logger_handler = logging.StreamHandler()
        logger_handler.setLevel(log_level)
        logger_handler.setFormatter(
            logging.Formatter(f"[{name}] %(asctime)s - %(levelname)s - %(message)s")
        )

        logger = logging.getLogger(name)
        logger.setLevel(log_level)
        logger.addHandler(logger_handler)

        return logger