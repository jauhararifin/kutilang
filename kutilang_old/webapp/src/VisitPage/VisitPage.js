import { ContentState, convertToRaw, EditorState } from 'draft-js';
import draftToHtml from 'draftjs-to-html';
import { useFormik } from 'formik';
import htmlToDraft from 'html-to-draftjs';
import moment from 'moment';
import 'moment/locale/id';
import Noty from 'noty';
import React, { useEffect, useRef, useState } from 'react';
import { useReactToPrint } from 'react-to-print';
import Button from 'react-bootstrap/Button';
import Card from 'react-bootstrap/Card';
import Container from 'react-bootstrap/Container';
import Form from 'react-bootstrap/Form';
import InputGroup from 'react-bootstrap/InputGroup';
import Row from 'react-bootstrap/Row';
import Spinner from 'react-bootstrap/Spinner';
import Table from 'react-bootstrap/Table';
import { Editor } from "react-draft-wysiwyg";
import "react-draft-wysiwyg/dist/react-draft-wysiwyg.css";
import { FaPrint } from "react-icons/fa";
import { BsPencil, BsTrash } from "react-icons/bs";
import { useHistory, useParams } from 'react-router-dom';
import * as Yup from 'yup';
import { deletePatient, deleteVisit, getPatient, getPatientVisits, insertVisit, postPatient, putPatient, putVisit } from '../service';

function showSuccessPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'success', theme: 'bootstrap-v4', timeout: 3000 }).show();
}

function showErrorPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'error', theme: 'bootstrap-v4', timeout: 3000 }).show();
}

function VisitPage() {
	const history = useHistory();
	const { patientId } = useParams()
	const isNewPatientPage = !patientId
	const [isLoading, setIsLoading] = useState(false)

	const [patient, setPatient] = useState(null)
	const [patientVisits, setPatientVisits] = useState([])
	const [nextCursor, setNextCursor] = useState('')

	useEffect(async () => {
		if (!patientId) return
		setIsLoading(true)

		const result = await getPatient(patientId)
		if (result['errorMessage'])
			showErrorPopup(result['errorMessage'])
		else
			setPatient(result['result'])

		const visitResp = await getPatientVisits(patientId, '')
		if (visitResp['errorMessage'])
			showErrorPopup(visitResp['errorMessage'])
		else {
			setPatientVisits(visitResp['result']['results'])
			setNextCursor(visitResp['result']['nextCursor'])
		}

		setIsLoading(false)
	}, [patientId])

	const nextPatientVisits = async () => {
		const visitResp = await getPatientVisits(patientId, nextCursor)
		if (visitResp['errorMessage'])
			showErrorPopup(visitResp['errorMessage'])
		else {
			setPatientVisits([...patientVisits, ...visitResp['result']['results']])
			setNextCursor(visitResp['result']['nextCursor'])
		}
	}

	const [isEditingProfile, setIsEditingProfile] = useState(false)

	const createProfile = async (patient) => {
		setIsLoading(true)
		const resp = await postPatient(patient)
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient added')
			history.replace(`/patients/${resp['result']['id']}`)
		}
		setIsLoading(false)
	}

	const updateProfile = async (newPatient) => {
		setIsLoading(true)
		const resp = await putPatient({ ...newPatient, id: patient.id })
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient updated')
			setIsEditingProfile(false)
			setPatient(newPatient)
		}
		setIsLoading(false)
	}

	const editProfile = () => { setIsEditingProfile(true) }

	const deleteProfile = async () => {
		if (!window.confirm('Are you sure you want to delete this patient?'))
			return

		const resp = await deletePatient(patient)
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient deleted')
			history.replace('/')
		}
	}

	const newVisit = async ({ diagnosis }) => {
		setIsLoading(true)
		const resp = await insertVisit({
			patientId: patient.id,
			diagnosis: diagnosis,
		})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Visit inserted')
			setPatientVisits([resp['result'], ...patientVisits])
		}
		setIsLoading(false)
	}

	const updateVisit = async ({ id, diagnosis }) => {
		setIsLoading(true)
		const resp = await putVisit({
			id: id,
			patientId: patient.id,
			diagnosis: diagnosis,
		})
		let result = false
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			result = true
			showSuccessPopup('Visit updated')
			setPatientVisits(patientVisits.map(pv => pv.id == id ? resp['result'] : pv))
		}
		setIsLoading(false)
		return result
	}

	const removeVisit = async ({ id, patientId }) => {
		setIsLoading(true)
		const resp = await deleteVisit({
			id: id,
			patientId: patientId,
		})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Visit deleted')
			setPatientVisits(patientVisits.filter(pv => pv.id != id))
		}
		setIsLoading(false)
	}

	return <Container>
		{isLoading && <div style={{ textAlign: 'center' }}>
			<Spinner animation="border" role="status"><span className="visually-hidden">Loading...</span></Spinner>
		</div>}

		{isNewPatientPage && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, }}>New Profile</h5>
			<ProfileEditor patient={patient} onSubmit={createProfile} />
		</>}

		{(!isNewPatientPage && isEditingProfile) && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, }}>Update Profile</h5>
			<ProfileEditor patient={patient} onSubmit={updateProfile} />
		</>}

		{!isNewPatientPage && !!patient && !isEditingProfile && <>
			<Profile patient={patient} onEdit={editProfile} onDelete={deleteProfile} />
		</>}


		{!isNewPatientPage && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>New Entry</h5>
			{/* <VisitView onSubmit={newVisit} /> */}
			<Visit patient={patient} onSubmit={pv => newVisit(pv)} />

			{patientVisits.length > 0 && <h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>History</h5>}
			{patientVisits.map(patientVisit => <Visit
				key={patientVisit.id}
				patient={patient}
				patientVisit={patientVisit}
				onSubmit={pv => updateVisit({ ...pv, id: patientVisit.id })}
				onDelete={_ => removeVisit({ id: patientVisit.id, patientId: patientVisit.patientId })}
			/>)}

			{nextCursor && <Row style={{ marginBottom: 20 }}>
				<Button variant="link" onClick={nextPatientVisits}>Load more...</Button>
			</Row>}
		</>}
	</Container>
}

function Profile({ patient, onEdit, onDelete }) {
	return <Table striped bordered hover>
		<tbody>
			<tr>
				<th style={{ width: 200 }}>Name</th>
				<td>{patient.name}</td>
			</tr>
			<tr>
				<th>Family Name</th>
				<td>{patient.headFamilyName}</td>
			</tr>
			<tr>
				<th>Birthdate</th>
				<td>{moment(patient['birthDate']).format('LL')} ({moment().diff(patient['birthDate'], 'years')} years old)</td>
			</tr>
			<tr>
				<th>Address</th>
				<td>{patient['address']}</td>
			</tr>
			<tr>
				<th>Last Visit</th>
				<td>{moment(patient['lastVisit']).format('LL')} ({moment(patient['lastVisit']).fromNow()})</td>
			</tr>
			<tr>
				<td colSpan={2} style={{ textAlign: "right" }}>
					<Button variant="secondary" onClick={onEdit}>Edit</Button>{' '}
					<Button variant="danger" onClick={onDelete}>Delete</Button>
				</td>
			</tr>
		</tbody>
	</Table>
}

function ProfileEditor({ patient, onSubmit }) {
	patient = patient || {}
	const lastVisit = patient['lastVisit'] ? moment(patient['lastVisit']) : moment()
	const formik = useFormik({
		initialValues: {
			name: patient['name'] || '',
			headFamilyName: patient['headFamilyName'] || '',
			birthDate: patient['birthDate'] || '',
			address: patient['address'] || '',
		},
		validationSchema: Yup.object({
			name: Yup.string().max(255, 'Maximum at 255 character').required('Required'),
			headFamilyName: Yup.string().max(255, 'Maximum at 255 character'),
			birthDate: Yup.date().required().max(new Date(), 'Invalid date'),
			address: Yup.string().max(4096).required('Required'),
		}),
		onSubmit: onSubmit,
	});

	return <Form onSubmit={formik.handleSubmit}>
		<Table striped bordered hover>
			<tbody>
				<tr>
					<th style={{ width: 200 }}>Name</th>
					<td>
						<InputGroup>
							<Form.Control type="input" {...formik.getFieldProps('name')} isInvalid={formik.touched.name && formik.errors.name} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.name && formik.errors.name}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Family Name</th>
					<td>
						<InputGroup>
							<Form.Control type="input" {...formik.getFieldProps('headFamilyName')} isInvalid={formik.touched.headFamilyName && formik.errors.headFamilyName} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.headFamilyName && formik.errors.headFamilyName}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Birthdate</th>
					<td>
						<InputGroup>
							<Form.Control type="date" {...formik.getFieldProps('birthDate')} isInvalid={formik.touched.birthDate && formik.errors.birthDate} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.birthDate && formik.errors.birthDate}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Address</th>
					<td>
						<InputGroup>
							<Form.Control as="textarea" row={3} name="address" {...formik.getFieldProps('address')} isInvalid={formik.touched.address && formik.errors.address} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.address && formik.errors.address}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Last Visit</th>
					<td>{lastVisit.format('LL')}</td>
				</tr>
				<tr>
					<td colSpan={2} style={{ textAlign: "right" }}>
						<Button variant="success" type="submit">Save</Button>
					</td>
				</tr>
			</tbody>
		</Table>
	</Form>
}

function Visit({ onSubmit, onDelete, patient, patientVisit }) {
	const [isEditing, setIsEditing] = useState(false)

	const submit = async (...args) => {
		const success = await onSubmit(...args)
		if (success) setIsEditing(false)
	}

	if (!patientVisit) {
		return <VisitEditor onSubmit={submit} />
	}

	return isEditing ?
		<VisitEditor onSubmit={submit} onCancel={() => setIsEditing(false)} patientVisit={patientVisit} /> :
		<VisitView onDelete={onDelete} onEdit={() => setIsEditing(true)} patient={patient} patientVisit={patientVisit} />
}

function VisitView({ onDelete, onEdit, patient, patientVisit }) {
	const date = moment(patientVisit['date'])
	const dateStr = date.format('LL') + ' (' + date.fromNow() + ')'

	const printRef = useRef();
	const handlePrint = useReactToPrint({
		content: () => printRef.current,
	});

	return (
		<>
			<div style={{ display: "none" }}>
				{patientVisit && <VisitViewForPrint ref={printRef} patient={patient} patientVisit={patientVisit} />}
			</div>
			<Card style={{ marginBottom: 20 }}>
				<Card.Header>{dateStr}</Card.Header>

				<Card.Body>
					<div dangerouslySetInnerHTML={{ __html: patientVisit.diagnosis }}></div>
				</Card.Body>

				<Card.Footer style={{ textAlign: "right" }}>
					{patientVisit && <Button variant="link" onClick={handlePrint}><FaPrint /></Button>}
					<Button variant="link" onClick={onEdit}><BsPencil /></Button>
					<Button variant="link" onClick={onDelete}><BsTrash /></Button>
				</Card.Footer>
			</Card>
		</>
	)
}

class VisitViewForPrint extends React.PureComponent {
	render() {
		const date = moment(this.props.patientVisit['date'])
		const dateStr = date.format('LL')
		return (
			<div style={{ margin: 10 }}>
				<div style={{ width: "100%", border: "solid 1px", padding: 10 }} >
					<table>
						<tbody>
							<tr style={{ padding: 10 }}>
								<th>Nama</th>
								<td>{this.props.patient.name} / {this.props.patient.headFamilyName}</td>
							</tr>
							<tr style={{ padding: 10 }}>
								<th>Tanggal Lahir</th>
								<td>{moment(this.props.patient['birthDate']).format('LL')}</td>
							</tr>
							<tr style={{ padding: 10 }}>
								<th>Alamat</th>
								<td>{this.props.patient['address']}</td>
							</tr>
							<tr style={{ padding: 10 }}>
								<th>Kunjungan Terakhir</th>
								<td>{moment(this.props.patient['lastVisit']).format('LL')}</td>
							</tr>
						</tbody>
					</table>
				</div>
				<h5 style={{ textAlign: "center", paddingTop: 20 }}>{dateStr}</h5>
				<div style={{ marginTop: 20, padding: 5, fontSize: 12 }} dangerouslySetInnerHTML={{ __html: this.props.patientVisit.diagnosis }}></div>
			</div>
		);
	}
}

function VisitEditor({ onSubmit, onCancel, patientVisit }) {
	const defaultText = `
<h5>Subjektif</h5>
<p><strong>KU</strong>:</p>
<p><strong>RPS</strong>:</p>
<p><strong>RPD</strong>: Maag (-), HT (-), DM (-), Asma (-), Tipes (-), Alergi obat (-), Sakit jantung (-), minum jamu / obat bebas (-)</p>
<h5>Objektif</h5>
<ul>
	<li><strong>KU</strong>: Baik, CM. <strong>TD</strong>: / mmHg, <strong>HR</strong>: x/mnt reguler, <strong>BB</strong>: kg, <strong>TB</strong>: cm.</li>
	<li><strong>Konj</strong> anemis (-), <strong>Sclera</strong> Icterus (-). JVP R-2 cm, <strong>Kel. tiroid</strong>: dbn</li>
	<li><strong>Thorax</strong>: SSD, SF: ka = ki. <strong>Pulmo</strong>: SD vesikuler, Ronchi: -, Wheezing: -.</li>
	<li><strong>Cor</strong>: konfigurasi dbn, S1-S2 murni, bising (-), gallop (-).</li>
	<li><strong>Abd</strong>: BU (+), venectasi (-), asites (-), <strong>Hepar</strong>: ttb, <strong>Lien</strong>: ttb. <strong>Nyeri ketok CF:</strong>ka (-) ki (-)</li>
	<li><strong>Extr</strong>: edema -/-, Ulcus -/-, tremor (-), Palmar eritem (-). jari tabuh (-). ptechiae (-).</li>
	<li><strong>GDS</strong>: mg/ml, <strong>AU</strong>: mg/ml, <strong>Kol. Total</strong>: mg/ml, <strong>EKG</strong>:
</ul>
<h5>Assesment</h5>
<ol>
	<li></li>
</ol>
<h5>Planning</h5>
<ol>
	<li></li>
</ol>
`

	const initialText = patientVisit ? EditorState.createWithContent(
		ContentState.createFromBlockArray(
			htmlToDraft(patientVisit.diagnosis).contentBlocks
		)
	) : EditorState.createWithContent(
		ContentState.createFromBlockArray(
			htmlToDraft(defaultText).contentBlocks
		)
	)

	const [editorState, setEditorState] = useState(initialText);

	const date = !!patientVisit ? moment(patientVisit.date) : moment()
	const dateStr = !!patientVisit ? date.format('LL') + ' (' + date.fromNow() + ')' : date.format('LL')

	const submit = () => {
		onSubmit({
			'diagnosis': draftToHtml(convertToRaw(editorState.getCurrentContent())),
		})
		setEditorState(initialText)
	}
	return (
		<Card style={{ marginBottom: 20 }}>
			<Card.Header>{dateStr}</Card.Header>

			<Card.Body style={{ padding: 0 }}>
				<Editor
					editorState={editorState}
					onEditorStateChange={setEditorState}
					toolbar={{ options: ['inline', 'blockType', 'fontSize', 'list', 'textAlign', 'colorPicker', 'link', 'emoji', 'remove', 'history'], }}
					toolbarStyle={{ border: 0, padding: 0 }}
					editorStyle={{ padding: 10 }}
				/>
			</Card.Body>

			<Card.Footer style={{ textAlign: "right" }}>
				{!!patientVisit && <Button variant="secondary" onClick={onCancel}>Cancel</Button>}
				{' '}<Button variant="primary" onClick={submit}>Submit</Button>
			</Card.Footer>
		</Card>
	)
}

export default VisitPage;
