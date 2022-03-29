export default function (api) {
  // Cache configuration is a required option
  api.cache(false);

  const presets = ['@babel/preset-typescript', '@babel/preset-env'];
  const plugins = ['@babel/plugin-transform-modules-commonjs']

  return { 
    presets,
    plugins,
  };
}
