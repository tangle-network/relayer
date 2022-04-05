export type CamelToKebabCase<S extends string> =
  S extends `${infer T}${infer U}`
    ? `${T extends Capitalize<T>
        ? '-'
        : ''}${Lowercase<T>}${CamelToKebabCase<U>}`
    : S;

export type ConvertToKebabCase<T> = {
  [P in keyof T as Lowercase<CamelToKebabCase<string & P>>]: T[P];
};
