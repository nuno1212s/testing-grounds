# How to use

    # Start a TCP server listening on port 4321
    $ socat tcp-l:4321,fork open:/dev/null

    # In a separate shell window run this project
    $ cargo run
