from pathlib import Path
import sys

from sqlmodel import Session

from .database import engine, seed

def main():
    command = sys.argv[1]

    match command:
        case "seed":
            cli_seed(sys.argv[2])

def cli_seed(path: str):
    with Session(engine) as session:
        seed(session, Path(path), True)
        session.commit()

if __name__ == "__main__":
    main()