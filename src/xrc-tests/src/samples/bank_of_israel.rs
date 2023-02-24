use chrono::Utc;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<message:StructureSpecificData
    xmlns:ss="http://www.sdmx.org/resources/sdmxml/schemas/v2_1/data/structurespecific"
    xmlns:footer="http://www.sdmx.org/resources/sdmxml/schemas/v2_1/message/footer"
    xmlns:ns1="urn:sdmx:org.sdmx.infomodel.datastructure.Dataflow=BOI.STATISTICS:EXR(1.0):ObsLevelDim:TIME_PERIOD"
    xmlns:message="http://www.sdmx.org/resources/sdmxml/schemas/v2_1/message"
    xmlns:common="http://www.sdmx.org/resources/sdmxml/schemas/v2_1/common"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xsi:schemaLocation="http://www.sdmx.org/resources/sdmxml/schemas/v2_1/message https://registry.sdmx.org/schemas/v2_1/SDMXMessage.xsd
                        urn:sdmx:org.sdmx.infomodel.datastructure.Dataflow=BOI.STATISTICS:EXR(1.0):ObsLevelDim:TIME_PERIOD https://edge.boi.gov.il/FusionEdgeServer/ws/public/sdmxapi/rest/schema/dataflow/BOI.STATISTICS/EXR/1.0?format=sdmx-2.1">
    <message:Header>
        <message:ID>IDREF02d6adcb-98ad-43c8-9d7d-cc3f040d41e8</message:ID>
        <message:Test>false</message:Test>
        <message:Prepared>2023-01-18T14:24:37Z</message:Prepared>
        <message:Sender id="UNKNOWN"></message:Sender>
        <message:Receiver id="guest"></message:Receiver>
        <message:Structure structureID="BOI.STATISTICS_EXR_1_0"
            namespace="urn:sdmx:org.sdmx.infomodel.datastructure.Dataflow=BOI.STATISTICS:EXR(1.0):ObsLevelDim:TIME_PERIOD"
            dimensionAtObservation="TIME_PERIOD">
            <common:StructureUsage>
                <Ref agencyID="BOI.STATISTICS" id="EXR" version="1.0"></Ref>
            </common:StructureUsage>
        </message:Structure>
        <message:DataSetAction>Information</message:DataSetAction>
        <message:Extracted>2023-01-18T14:24:37</message:Extracted>
        <message:ReportingBegin>2023-01-05T00:00:00</message:ReportingBegin>
        <message:ReportingEnd>2023-01-05T23:59:59</message:ReportingEnd>
    </message:Header>
    <message:DataSet ss:dataScope="DataStructure" xsi:type="ns1:DataSetType"
        ss:structureRef="BOI.STATISTICS_EXR_1_0">
        <Series SERIES_CODE="RER_GBP_ILS" FREQ="D" BASE_CURRENCY="GBP" COUNTER_CURRENCY="ILS"
            UNIT_MEASURE="ILS" DATA_TYPE="OF00" TIME_COLLECT="V" DATA_SOURCE="BOI_MRKT"
            UNIT_MULT="0" CONF_STATUS="F" PUB_WEBSITE="Y">
            <Obs TIME_PERIOD="{{DATE}}" OBS_VALUE="4.2376"></Obs>
        </Series>
        <Series SERIES_CODE="RER_EUR_ILS" FREQ="D" BASE_CURRENCY="EUR" COUNTER_CURRENCY="ILS"
            UNIT_MEASURE="ILS" DATA_TYPE="OF00" TIME_COLLECT="V" DATA_SOURCE="BOI_MRKT"
            UNIT_MULT="0" CONF_STATUS="F" PUB_WEBSITE="Y">
            <Obs TIME_PERIOD="{{DATE}}" OBS_VALUE="3.7439"></Obs>
        </Series>
        <Series SERIES_CODE="RER_JPY_ILS" FREQ="D" BASE_CURRENCY="JPY" COUNTER_CURRENCY="ILS"
            UNIT_MEASURE="ILS" DATA_TYPE="OF00" TIME_COLLECT="V" DATA_SOURCE="BOI_MRKT"
            UNIT_MULT="2" CONF_STATUS="F" PUB_WEBSITE="Y">
            <Obs TIME_PERIOD="{{DATE}}" OBS_VALUE="2.6603"></Obs>
        </Series>
        <Series SERIES_CODE="RER_USD_ILS" FREQ="D" BASE_CURRENCY="USD" COUNTER_CURRENCY="ILS"
            UNIT_MEASURE="ILS" DATA_TYPE="OF00" TIME_COLLECT="V" DATA_SOURCE="BOI_MRKT"
            UNIT_MULT="0" CONF_STATUS="F" PUB_WEBSITE="Y">
            <Obs TIME_PERIOD="{{DATE}}" OBS_VALUE="3.529"></Obs>
        </Series>
    </message:DataSet>
</message:StructureSpecificData>"#;

pub fn bank_of_israel(date: &chrono::DateTime<Utc>) -> Vec<u8> {
    TEMPLATE
        .replace("{{DATE}}", &date.format("%Y-%m-%d").to_string())
        .into_bytes()
}
