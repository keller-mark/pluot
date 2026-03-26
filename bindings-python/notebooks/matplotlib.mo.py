import marimo

__generated_with = "0.18.4"
app = marimo.App(width="medium")


@app.cell
def _():
    import matplotlib.pyplot as plt
    return (plt,)


@app.cell
def _():
    import numpy as np
    return (np,)


@app.cell
def _(np):
    x_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
    y_arr = ((np.random.rand(500) - 0.5) * 10.0).astype('<f8')
    color_arr = np.array(
      [5, 6, 7, 6, 8] * 100
    ).astype('<i8')
    return color_arr, x_arr, y_arr


@app.cell
def _(color_arr, plt, x_arr, y_arr):
    fig, ax = plt.subplots(figsize=(8, 6))

    sc = ax.scatter(x_arr, y_arr, c=color_arr, cmap='viridis', alpha=0.7, edgecolors='none', s=30)

    cbar = plt.colorbar(sc, ax=ax)
    cbar.set_label('Color Value')

    ax.set_xlabel('X')
    ax.set_ylabel('Y')
    ax.set_title('Scatterplot')

    plt.tight_layout()
    plt.show()
    return


@app.cell
def _():
    return


if __name__ == "__main__":
    app.run()
