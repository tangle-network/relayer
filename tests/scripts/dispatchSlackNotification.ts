let opsgenie = require('opsgenie-sdk');
require('dotenv').config({ path: '.env' });

opsgenie.configure({
  'host': 'https://api.opsgenie.com',
  'api_key': `${process.env.OPS_GENIE_API}`
});

export const create_slack_alert = (message: string, description: string = '') => {
  const slackAlert = {
    "message": message,
    "responders":[
      {
          "name":"WebbRelayer",
          "type":"team"
      }
    ],
    "description": description
  };

  opsgenie.alertV2.create(slackAlert, function (error, alert) {
    if (error) {
        console.error(error);
    } else {
        console.log(alert);
    }
  });
}
