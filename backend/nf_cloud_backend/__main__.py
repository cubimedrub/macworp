from pathlib import Path
import sys

from sqlmodel import Session

from .database import engine, seed

def main():
    command = sys.argv[1]

    match command:
        case "seed":
            seed(Path(sys.argv[2]))

if __name__ == "__main__":
    main()