/*
 * Copyright 2022 Webb Technologies Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
class JsStorage {
  constructor() {
    this.db = {};
  }

  get(key) {
    return this.db[key];
  }

  get_or_element(key, defaultElement) {
    const element = this.db[key];
    if (element === undefined) {
      return defaultElement;
    } else {
      return element;
    }
  }

  put(key, value) {
    if (key === undefined || value === undefined) {
      throw Error('key or value is undefined');
    }
    this.db[key] = value;
  }

  del(key) {
    delete this.db[key];
  }

  put_batch(key_values) {
    key_values.forEach((element) => {
      this.db[element.key] = element.value;
    });
  }
}

export default JsStorage;
