import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import sys
import matplotlib as mpl
mpl.rcParams['agg.path.chunksize'] = 10000


def calcCDF(data):
    x = np.sort(np.array(data))
    y = np.arange(1, len(x)+1)/len(x)
    return x, y

def process_ccdf(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    x, y = calcCDF(data['rtt'])
    ccdf = 1 - y
    return x, y, ccdf

def process_pist_ccdf(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()
    pist = np.diff(data['send_timestamp']/1e6)
    
    x, y = calcCDF(pist)
    ccdf = 1 - y
    return x, ccdf

def process_time_series(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    time = data['send_timestamp']
    

    x = data['seq']
    y = data['rtt']
    return x, y

def process_pist(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    pist = np.diff(data['send_timestamp'])
    return pist/1e6

def process_seq(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    seq = data['seq']
    seq = np.diff(seq)
    # Find the indecies of the sequence number jumps
    seq_diff = np.where(seq != 1)
    print(seq_diff)
    return seq


if __name__ == "__main__":
    args = sys.argv
    if len(args) < 2:
        print("Usage: python process.py <data_paths> space separated")
        sys.exit(1)

    data_paths = args[1:]
    samples = []
    fig_ccdf, ax_ccdf = plt.subplots(figsize=(16, 9), layout='tight')
    fig_time, ax_time = plt.subplots(figsize=(16, 9), layout='tight')
    fig_pist, ax_pist = plt.subplots(figsize=(16, 9), layout='tight')
    fig_pist_ccdf, ax_pist_ccdf = plt.subplots(figsize=(16, 9), layout='tight')
    for data_path in data_paths:
        x, cdf, ccdf = process_ccdf(data_path)
        seq, rtt = process_time_series(data_path)
        pist = process_pist(data_path)
        x_pist, ccdf_pist = process_pist_ccdf(data_path)
        seq_diff = process_seq(data_path)
        print(seq_diff)


        ax_pist_ccdf.plot(x_pist, ccdf_pist, label=data_path)
        ax_ccdf.plot(x, ccdf, label=data_path)
        ax_time.plot(seq,rtt, label=data_path)
        ax_pist.plot(pist, label=data_path)
        samples.append(x)
    
    ax_ccdf.set_xlabel('RTT [ms]')
    ax_ccdf.set_ylabel('CCDF [-]')
    ax_ccdf.set_yscale('log')

    ax_pist_ccdf.set_xlabel('pist [ms]')
    ax_pist_ccdf.set_ylabel('CCDF [-]')
    ax_pist_ccdf.set_yscale('log')

    ax_time.set_xlabel('Sample number [-]')
    ax_time.set_ylabel('RTT [ms]')
    ax_time.grid(True)

    ax_pist.set_xlabel('[-]')
    ax_pist.set_ylabel('pist [ms]')
    ax_pist.grid(True)


    min_sample = min(samples, key=len)
    min_sample = len(min_sample)
    ax_ccdf.set_ylim([10**-np.log10(min_sample/10), 1])

    fig_ccdf.legend()
    ax_ccdf.grid(True, which="both", ls="-")
    fig_ccdf.savefig('ccdf.png', dpi=300)
    fig_ccdf.clf()


    fig_time.legend()
    ax_time.grid(True, which="both", ls="-")
    fig_time.savefig('time.png', dpi=300)
    fig_time.clf()

    fig_pist.legend()
    ax_pist.grid(True, which="both", ls="-")
    fig_pist.savefig('pist.png', dpi=300)
    fig_pist.clf()

    fig_pist_ccdf.legend()
    ax_pist_ccdf.grid(True, which="both", ls="-")
    fig_pist_ccdf.savefig('pist_ccdf.png', dpi=300)
    fig_pist_ccdf.clf()

