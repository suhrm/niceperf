import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import sys



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

def process_piat_ccdf(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()
    piat = np.diff(data['send_timestamp']/1e6)
    
    x, y = calcCDF(piat)
    ccdf = 1 - y
    return x, ccdf

def process_time_series(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    x = data['seq']
    y = data['rtt']
    return x, y

def process_piat(data_path: str):
    data = pd.read_csv(data_path)
    data = data.dropna()

    piat = np.diff(data['send_timestamp'])
    return piat/1e6




if __name__ == "__main__":
    args = sys.argv
    if len(args) != 2:
        print("Usage: python process.py <data_paths> space separated")
        sys.exit(1)

    data_paths = args[1:]
    samples = []
    fig_ccdf, ax_ccdf = plt.subplots(figsize=(16, 9), layout='tight')
    fig_time, ax_time = plt.subplots(figsize=(16, 9), layout='tight')
    fig_piat, ax_piat = plt.subplots(figsize=(16, 9), layout='tight')
    fig_piat_ccdf, ax_piat_ccdf = plt.subplots(figsize=(16, 9), layout='tight')
    for data_path in data_paths:
        x, cdf, ccdf = process_ccdf(data_path)
        seq, rtt = process_time_series(data_path)
        piat = process_piat(data_path)
        x_piat, ccdf_piat = process_piat_ccdf(data_path)
        ax_piat_ccdf.plot(x_piat, ccdf_piat, label=data_path)
        ax_ccdf.plot(x, ccdf, label=data_path)
        ax_time.plot(seq,rtt, label=data_path)
        ax_piat.plot(piat, label=data_path)
        samples.append(x)
    
    ax_ccdf.set_xlabel('RTT [ms]')
    ax_ccdf.set_ylabel('CCDF [-]')
    ax_ccdf.set_yscale('log')

    ax_piat_ccdf.set_xlabel('PIAT [ms]')
    ax_piat_ccdf.set_ylabel('CCDF [-]')
    ax_piat_ccdf.set_yscale('log')

    ax_time.set_xlabel('Sample number [-]')
    ax_time.set_ylabel('RTT [ms]')
    ax_time.grid(True)

    ax_piat.set_xlabel('[-]')
    ax_piat.set_ylabel('PIAT [ms]')
    ax_piat.grid(True)


    min_sample = min(samples, key=len)
    min_sample = len(min_sample)
    ax_ccdf.set_ylim([10**-np.log10(min_sample), 1])

    fig_ccdf.legend()
    ax_ccdf.grid(True, which="both", ls="-")
    fig_ccdf.savefig('ccdf.png', dpi=300)
    fig_ccdf.clf()


    fig_time.legend()
    ax_time.grid(True, which="both", ls="-")
    fig_time.savefig('time.png', dpi=300)
    fig_time.clf()

    fig_piat.legend()
    ax_piat.grid(True, which="both", ls="-")
    fig_piat.savefig('piat.png', dpi=300)
    fig_piat.clf()

    fig_piat_ccdf.legend()
    ax_piat_ccdf.grid(True, which="both", ls="-")
    fig_piat_ccdf.savefig('piat_ccdf.png', dpi=300)
    fig_piat_ccdf.clf()

