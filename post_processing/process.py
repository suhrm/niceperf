import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import sys



def calcCDF(data):
    x = np.sort(np.array(data))
    y = np.arange(1, len(x)+1)/len(x)
    return x, y

def process_data(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    x, y = calcCDF(data['rtt'])
    ccdf = 1 - y
    return x, y, ccdf

if __name__ == "__main__":
    args = sys.argv
    if len(args) != 2:
        print("Usage: python process.py <data_paths> space separated")
        sys.exit(1)

    data_paths = args[1:]
    samples = []
    ax = plt.figure(figsize=(16, 9), layout='tight')
    for data_path in data_paths:
        x, cdf, ccdf = process_data(data_path)
        plt.plot(x, ccdf, label=data_path)
        samples.append(x)
    
    plt.xlabel('RTT [ms]')
    plt.ylabel('CCDF [-]')
    plt.yscale('log')
    # plt.xscale('log')


    min_sample = min(samples, key=len)
    min_sample = len(min_sample)
    plt.ylim([10**-np.log10(min_sample), 1])

    plt.legend()

    plt.grid(True, which="both", ls="-")
    plt.savefig('ccdf.png', dpi=300)
