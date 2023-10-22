import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import sys

if __name__ == "__main__":
    
    args = sys.argv
    if len(args) != 2:
        print("Usage: python trace_cmd.py <data_paths> space separated")
        sys.exit(1)

    data_paths = args[1:]
    samples = []

    for data_path in data_paths:
        data = np.loadtxt(data_path)
        data = np.diff(data)*1e3
        plt.plot(data)

    plt.show()
