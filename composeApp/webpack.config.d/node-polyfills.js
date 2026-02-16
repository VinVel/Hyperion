const webpack = require("webpack");

config.resolve.fallback = {
    ...(config.resolve.fallback || {}),
    crypto: require.resolve("crypto-browserify"),
    buffer: require.resolve("buffer/"),
    stream: require.resolve("stream-browserify"),
    events: require.resolve("events/"),
    process: require.resolve("process/browser.js"),
    util: require.resolve("util/"),
    assert: require.resolve("assert/")
};

config.plugins = (config.plugins || []).concat([
    new webpack.ProvidePlugin({
        Buffer: ["buffer", "Buffer"],
        process: "process/browser.js"
    })
]);
