export default {
  extensionsToTreatAsEsm: ['.ts'],
  transformIgnorePatterns: [
    '\\.pnp\\.[^\\/]+$',
    '/node_modules/(?!@polkadot|@babel/runtime/helpers/esm/)',
  ],
};
