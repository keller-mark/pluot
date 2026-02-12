import logging

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'

def get_logger():
    logging.basicConfig(format=FORMAT)
    logger = logging.getLogger('pluot')
    logger.setLevel(logging.INFO)
    return logger
