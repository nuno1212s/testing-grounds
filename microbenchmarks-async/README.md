# Microbenchmarks with asynchronous bounded clients

This benchmark is designed to measure the performance of the asynchronous bounded clients with a placeholder application.

Each client will run a max of `X` concurrent requests at a time, and will make `Y` requests in total.

Both of these are configurable in the `benchmark_config.toml` file.

The benchmark will measure the time taken to make all requests, and the average time taken to make a single request.

# Setup influx DB

## Setup your own instance of influx wherever you want.

Recommended version is 1.8.3

## Edit the influx db configuration file

This should contain the correct access data for influx DB.
If this is not correct, the benchmark might not work.

# Docker

We do have a docker setup but the image is not very up to date so you would have to build your own image.

TODO: Improve documentation on this.

# Running the benchmark

## Install rust and all the necessary dependencies

## Setup the environment

To run the benchmark, we need `3f+1` separate terminals, where `f` is the number of failures we want to tolerate.

Firstly, setup all the necessary environment variables for this benchmark:

If you are using the config files provided here, you will probably just need:
```bash
export ID=0
export OWN_NODE__NODE_ID=0
export OWN_NODE__IP=127.0.0.1
export OWN_NODE__HOSTNAME=srv-0
export OWN_NODE__NODE_TYPE=Replica
```

## Edit all the necessary configuration files

Check the configuration files and edit them to match your desired setup.
Especially the `nodes.toml` file, which contains a description of the network.

## Run the benchmark

```bash 
./run servers
```
