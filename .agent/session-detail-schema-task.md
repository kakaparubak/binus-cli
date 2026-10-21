# Approved next slice prerequisite: session detail metadata schema

Userapprovedcontinuingwithproposedsessiondetails/topictext/materialmetadata afterSessionslistaccepted. Work /home/abdel/Documents/Coding/binus-cli, gpt-6-luna. PriorCourses+Sessionsaccepted50lib+9CLI+2exampletests,fm/clippy/release; parentlive13LEC6LABsessions. Parentfreshauthchecksuccess3periods. DoNOTredoauth/login/refresh; sandbox /homeuid65534 securityissuehandledbyparenthostreads. No realcredential/APIaccessinsideCodex, no weakenownership, no copytokens, no commits/push/sourcewrites.

IMMEDIATE task ONLY: build small structural schemahelper `spikes/001-auth/examples/session_detail_schema.rs` to support upcoming `binus courses sessions show <session-id>`. ParentwillpickfirstsessionfromacceptedsessionsCLI forselectedLEC/LABclasses, executehelperoutside sandbox, relaysanitizedshapesbeforeproductionparser. Noactualproductionstubcommandyet.

Verifiedallowlistedcourseorigin https://apim-bm7-prod-web.azure-api.net/func-bm7-course-prod
Two exactknownreadroutes:
GET /ClassSession/Session/{sessionId}/Resource/Student?isWeb=true
GET /ClassSession/Session/{sessionId}/AdditionalResources
NootherGETs/POST/PUT/PATCH/DELETE, noresourceURLfollowing/download. Existingresearchretrievedtopic/timing/delivery/resourceidentifiers fromprimaryandadditionalmetadata; exactJSONschemamustbeobserved, notguessed. Don'tauto-call /ClassSession/{id}/Session/Resource/{resourceId} or assessment/forum routes inthishelper.

CLIhelper `--session <id> --kind primary|additional`, validIDbeforesavedcredload, --help noI/O. Reuseprotectedload_preferred_record andacademic_period_request headersinprocess. Requestrewrites ONLY fixedoriginpath withencodedsegments and forprimarysets literalquery isWeb=true (nonsecret documentedrequiredflag); additionalnoquery. SameBearer+5roleheaders, noCookie/body/subscriptionquery, Policy::none. ONEabsolute20srequest acrossheadersandALLbodychunks,30soverall, streamed8MiBcap, errors401403authvs500upstream. Nobrowser/realHTTPinsandbox. Parentcanexecute4metadatareads total(twoendpointsperselectedsession) usingbuiltdebugexample.

Output ONEJSONstructuralreportonly: HTTPstatus, roottypes/counts, all unionfieldnames/types, namednestedarraypaths/countsets, pagination/totalkeytypes. Don'tselectfirstnestedarrayasrecords; therecanbelecturers andotherarrays. DoNOTprint scalarvalues: topictext/names/studentIDs/realresourceIDs/URLs/signedlocators/authstate. Schemapath/fieldnames/countsonly. Noneofrawresponseswrittentorepo/vault/logs. Class/sessionIDs stayparentargs/inmemory, notoutput. Genericwalkaggregateallarrayelementtypes,notonlyfirst. Canreuseactual session_schema walker but FIXrecordfieldtypes firstarrayheuristicbyexplicitnoassumptions; unionpathsandcounts sufficient. No pointlesswholebrowser/sourceinvestigation. Publiccurrentfrontendcallsiteinspectionifneededcanfollowlaterwithshapes.

Synthetictestsassertprivatevalues/URLstringsnotinserializedschema, unsafeIDrejects, desirednamedarraypaths/classnotmixedwithlecturers. Buildhelper, cargo test --example session_detail_schema,fmt --check,clippyalltargets-Dwarnings; allcargo testregressionsshouldstaygreen. Noauth/storage/Courses/Sessionsproductioncodechangesneeded. Ifbaselineflakeobservedreportexacttestbutdon'tclaimprodregression/fixwholeapp; priorfragilecoursetimerwasstabilizedalready.

Evidence `.agent/session-detail-schema-evidence.md`, separateCLI-o final `.agent/session-detail-schema-summary.md` toavoidreportoverwrite. Includeexacthostruncommandsandlimits. STOPafterhelperbuilt; parentrunsrealreadsnotsandbox, thenhandsbackschemasforactualshowimplementation. ScopeNOdownload/SQLite/offline/renewal/forum/assessment. No commits/push andmodelremainsgpt-6-luna. Honestreportcommands/testresultsnotinventedliveevidence.
