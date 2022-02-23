/** @type {import('ts-jest/dist/types').InitialOptionsTsJest} */
module.exports = {
	preset: 'ts-jest',
    slowTestThreshold: 200,
	testEnvironment: 'node',
	setupFilesAfterEnv: ['jest-extended/all'],
	extensionsToTreatAsEsm: ['.ts'],
	globals: {
		'ts-jest': {
			useESM: true,
		},
	},
};
