#!/usr/bin/env python

"""Generates bar blots for health expectancy & expanses by country for 2021. 
"""

# std imports
import json
from pathlib import Path
import sys


# 3rd party imports
import pandas as pd
import plotly.graph_objects as go  # type: ignore


def main():
    df = pd.read_csv(sys.argv[1])

    df_2021 = df[df["Year"] == 2021]

    ## Table
    df_2021.to_csv("01_healthexp_2021.csv", index=False)

    ## Gen bar plot for pu
    plot = go.Figure()
    plot.add_trace(
        go.Bar(
            x=df_2021["Country"],
            y=df_2021["Life_Expectancy"],
            name="Life expectancy",
        ),
    )
    plot.add_trace(
        go.Bar(
            x=df_2021["Country"],
            y=df_2021["Spending_USD"],
            name="Spending (USD)",
        ),
    )

    plot.update_layout(
        barmode="group",
    )

    # Image
    plot.write_image("02_healthexp_2021.png")

    ## Embedded SVG
    plot.write_image("03_healthexp_2021.svg")
    with Path("03_healthexp_2021.svg.mmdata").open(
        "w", encoding="utf-8"
    ) as mmdata_file:
        mmdata_file.write(
            json.dumps(
                {
                    "description": "SVG embedded into the HTML DOM",
                }
            )
        )

    ## SVG in image tag
    plot.write_image("04_healthexp_2021.image.svg")
    with Path("04_healthexp_2021.image.svg.mmdata").open(
        "w", encoding="utf-8"
    ) as mmdata_file:
        mmdata_file.write(
            json.dumps(
                {
                    "header": "Header overwritten with mmdata 'header' attribute",
                    "description": "Same SVG as above but put into an image tag",
                }
            )
        )
    plot.write_image("05_healthexp_2021.pdf")

    ## Interactive plot
    plot.write_json("06_healthexp_2021.plotly.json")
    with Path("06_healthexp_2021.plotly.json.mmdata").open(
        "w", encoding="utf-8"
    ) as mmdata_file:
        mmdata_file.write(
            json.dumps(
                {
                    "header": "Interactive plot",
                    "description": "Lorem Ipsum",
                }
            )
        )

    df_2021.to_csv("07_healthexp_2021.txt", index=False)
    with Path("07_healthexp_2021.txt.mmdata").open(
        "w", encoding="utf-8"
    ) as mmdata_file:
        mmdata_file.write(
            json.dumps(
                {
                    "header": "Same as CSV but with .txt extension",
                    "description": "Simple txt viewer",
                }
            )
        )


if __name__ == "__main__":
    main()
